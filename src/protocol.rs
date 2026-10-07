use crate::player::{Command, Request, Snapshot};
use anyhow::Result;
use axum::{
    Router,
    extract::{Path, State},
    http::{HeaderMap, Method, StatusCode},
    response::{IntoResponse, Response},
    routing::{any, get},
};
use quick_xml::escape::escape;
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::{Mutex, mpsc, oneshot, watch};

const AV: &str = "AVTransport";
const RC: &str = "RenderingControl";
const CM: &str = "ConnectionManager";
const SINK: &str = "http-get:*:video/mp4:*,http-get:*:video/mpeg:*,http-get:*:video/x-matroska:*,http-get:*:application/vnd.apple.mpegurl:*,http-get:*:application/x-mpegURL:*,http-get:*:video/*:*,http-get:*:audio/*:*";
pub struct Receiver {
    commands: mpsc::Sender<Request>,
    states: watch::Receiver<Snapshot>,
    name: String,
    id: uuid::Uuid,
    subscriptions: Mutex<HashMap<String, Subscription>>,
    client: reqwest::Client,
    action_lock: Mutex<()>,
    delivery_lock: Mutex<()>,
}
struct Subscription {
    service: String,
    callback: String,
    expires: Instant,
    sequence: u32,
    failures: u8,
}
impl Receiver {
    pub fn new(
        commands: mpsc::Sender<Request>,
        states: watch::Receiver<Snapshot>,
        name: String,
        id: uuid::Uuid,
    ) -> Self {
        Self {
            commands,
            states,
            name,
            id,
            subscriptions: Mutex::new(HashMap::new()),
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(3))
                .build()
                .expect("HTTP client"),
            action_lock: Mutex::new(()),
            delivery_lock: Mutex::new(()),
        }
    }
    async fn command(&self, command: Command) -> std::result::Result<(), Fault> {
        let (reply, result) = oneshot::channel();
        self.commands
            .send(Request { command, reply })
            .await
            .map_err(|_| Fault(501, "Receiver shutting down"))?;
        tokio::time::timeout(Duration::from_secs(5), result)
            .await
            .map_err(|_| Fault(501, "Player timeout"))?
            .map_err(|_| Fault(501, "Player unavailable"))?
            .map_err(|_| Fault(501, "Playback command failed"))
    }
    async fn notify(&self, only: Option<&str>) {
        let _delivery_order = self.delivery_lock.lock().await;
        let state = self.states.borrow().clone();
        let deliveries: Vec<_> = {
            let mut subscriptions = self.subscriptions.lock().await;
            subscriptions.retain(|_, s| s.expires > Instant::now());
            subscriptions
                .iter_mut()
                .filter(|(sid, _)| only.is_none_or(|target| *sid == target))
                .map(|(sid, s)| {
                    let seq = s.sequence;
                    s.sequence = if seq == u32::MAX { 1 } else { seq + 1 };
                    (
                        sid.clone(),
                        s.callback.clone(),
                        seq,
                        event_body(&s.service, &state),
                    )
                })
                .collect()
        };
        for (sid, callback, seq, body) in deliveries {
            // Never log callback URLs or media metadata.
            let result = self
                .client
                .request(reqwest::Method::from_bytes(b"NOTIFY").unwrap(), callback)
                .header("NT", "upnp:event")
                .header("NTS", "upnp:propchange")
                .header("SID", &sid)
                .header("SEQ", seq.to_string())
                .header("Content-Type", "text/xml; charset=\"utf-8\"")
                .body(body)
                .send()
                .await;
            if !matches!(result, Ok(ref r) if r.status().is_success()) {
                let mut subs = self.subscriptions.lock().await;
                if let Some(subscription) = subs.get_mut(&sid) {
                    subscription.failures += 1;
                }
                subs.retain(|_, subscription| subscription.failures < 3);
                tracing::debug!("Event delivery failed");
            } else if let Some(subscription) = self.subscriptions.lock().await.get_mut(&sid) {
                subscription.failures = 0;
            }
        }
    }
}

pub async fn serve(
    listener: tokio::net::TcpListener,
    receiver: Arc<Receiver>,
    mut shutdown: watch::Receiver<bool>,
) -> Result<()> {
    let events = receiver.clone();
    let mut event_shutdown = shutdown.clone();
    let event_task = tokio::spawn(async move {
        let mut states = events.states.clone();
        loop {
            tokio::select! {
                _ = event_shutdown.changed() => break,
                result = states.changed() => {
                    if result.is_err() {break;}
                    // Coalesce frequent position updates; subscriptions remain ordered.
                    tokio::time::sleep(Duration::from_millis(250)).await;
                    states.borrow_and_update();
                    events.notify(None).await;
                }
            }
        }
    });
    let app = Router::new()
        .route("/description.xml", get(description))
        .route("/service/{service}", get(service_description))
        .route("/control/{service}", any(control))
        .route("/event/{service}", any(subscription))
        .with_state(receiver);
    let result = axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            let _ = shutdown.changed().await;
        })
        .await;
    event_task.abort();
    result?;
    Ok(())
}
fn xml(body: String) -> Response {
    ([("Content-Type", "text/xml; charset=\"utf-8\"")], body).into_response()
}
async fn description(State(r): State<Arc<Receiver>>) -> Response {
    let services = [AV,RC,CM].map(|s|format!("<service><serviceType>urn:schemas-upnp-org:service:{s}:1</serviceType><serviceId>urn:upnp-org:serviceId:{s}</serviceId><SCPDURL>/service/{s}</SCPDURL><controlURL>/control/{s}</controlURL><eventSubURL>/event/{s}</eventSubURL></service>")).join("");
    xml(format!(
        "<?xml version=\"1.0\"?><root xmlns=\"urn:schemas-upnp-org:device-1-0\"><specVersion><major>1</major><minor>0</minor></specVersion><device><deviceType>urn:schemas-upnp-org:device:MediaRenderer:1</deviceType><friendlyName>{}</friendlyName><manufacturer>Castrivo</manufacturer><modelName>Castrivo</modelName><modelNumber>0.1</modelNumber><UDN>uuid:{}</UDN><serviceList>{services}</serviceList></device></root>",
        escape(&r.name),
        r.id
    ))
}
#[derive(Debug)]
struct Fault(u16, &'static str);
impl IntoResponse for Fault {
    fn into_response(self) -> Response {
        let response = xml(format!(
            "<s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\"><s:Body><s:Fault><faultcode>s:Client</faultcode><faultstring>UPnPError</faultstring><detail><UPnPError xmlns=\"urn:schemas-upnp-org:control-1-0\"><errorCode>{}</errorCode><errorDescription>{}</errorDescription></UPnPError></detail></s:Fault></s:Body></s:Envelope>",
            self.0, self.1
        ));
        (StatusCode::INTERNAL_SERVER_ERROR, response).into_response()
    }
}
fn field(name: &str, value: impl std::fmt::Display) -> String {
    format!("<{name}>{}</{name}>", escape(value.to_string()))
}
fn time(seconds: f64) -> String {
    let s = seconds.max(0.) as u64;
    format!("{:02}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
}
fn parse_time(value: &str) -> std::result::Result<f64, Fault> {
    let parts: Vec<_> = value.split(':').collect();
    if parts.len() != 3 {
        return Err(Fault(402, "Invalid Args"));
    }
    let h = parts[0]
        .parse::<u32>()
        .map_err(|_| Fault(402, "Invalid Args"))?;
    let m = parts[1]
        .parse::<u32>()
        .map_err(|_| Fault(402, "Invalid Args"))?;
    let s = parts[2]
        .parse::<f64>()
        .map_err(|_| Fault(402, "Invalid Args"))?;
    if m >= 60 || !s.is_finite() || !(0. ..60.).contains(&s) {
        return Err(Fault(402, "Invalid Args"));
    }
    Ok(f64::from(h) * 3600. + f64::from(m) * 60. + s)
}
struct Action {
    name: String,
    args: HashMap<String, String>,
}
impl Action {
    fn arg(&self, name: &str) -> std::result::Result<&str, Fault> {
        self.args
            .get(name)
            .map(String::as_str)
            .ok_or(Fault(402, "Invalid Args"))
    }
    fn parse(body: &str, service: &str) -> std::result::Result<Self, Fault> {
        let doc = roxmltree::Document::parse(body).map_err(|_| Fault(402, "Invalid Args"))?;
        let envelope = doc.root_element();
        let soap = "http://schemas.xmlsoap.org/soap/envelope/";
        if !envelope.has_tag_name((soap, "Envelope")) {
            return Err(Fault(402, "Invalid Args"));
        }
        let body = envelope
            .children()
            .find(|n| n.has_tag_name((soap, "Body")))
            .ok_or(Fault(402, "Invalid Args"))?;
        let node = body
            .children()
            .find(|n| n.is_element())
            .ok_or(Fault(402, "Invalid Args"))?;
        if node.tag_name().namespace()
            != Some(format!("urn:schemas-upnp-org:service:{service}:1").as_str())
        {
            return Err(Fault(401, "Invalid Action"));
        }
        Ok(Self {
            name: node.tag_name().name().into(),
            args: node
                .children()
                .filter(|n| n.is_element())
                .map(|n| (n.tag_name().name().into(), n.text().unwrap_or("").into()))
                .collect(),
        })
    }
}
async fn control(
    State(r): State<Arc<Receiver>>,
    Path(service): Path<String>,
    method: Method,
    headers: HeaderMap,
    body: String,
) -> Response {
    if method != Method::POST {
        return StatusCode::METHOD_NOT_ALLOWED.into_response();
    }
    let result=async {
        if ![AV,RC,CM].contains(&service.as_str()) {return Err(Fault(401,"Invalid Action"));}
        let action=Action::parse(&body,&service)?;
        let expected=format!("urn:schemas-upnp-org:service:{service}:1#{}",action.name);
        if headers.get("SOAPAction").and_then(|v|v.to_str().ok()).map(|v|v.trim_matches('"'))!=Some(expected.as_str()) {return Err(Fault(401,"Invalid Action"));}
        let _order=r.action_lock.lock().await;
        tracing::info!(service,action=action.name,"Receiver action");
        let inner=execute(&r,&service,&action).await?;
        Ok(xml(format!("<s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\" s:encodingStyle=\"http://schemas.xmlsoap.org/soap/encoding/\"><s:Body><u:{}Response xmlns:u=\"urn:schemas-upnp-org:service:{service}:1\">{inner}</u:{}Response></s:Body></s:Envelope>",action.name,action.name)))
    }.await;
    result.unwrap_or_else(IntoResponse::into_response)
}
async fn execute(r: &Receiver, service: &str, a: &Action) -> std::result::Result<String, Fault> {
    let schema = schemas(service);
    let (_, arguments) = schema
        .iter()
        .find(|(name, _)| *name == a.name)
        .ok_or(Fault(401, "Invalid Action"))?;
    for argument in arguments.split_whitespace() {
        let parts: Vec<_> = argument.split(':').collect();
        if parts[1] == "in" {
            a.arg(parts[0])?;
        }
    }
    let s = r.states.borrow().clone();
    if service != CM
        && a.arg("InstanceID")?
            .parse::<u32>()
            .map_err(|_| Fault(402, "Invalid Args"))?
            != 0
    {
        return Err(Fault(718, "Invalid InstanceID"));
    }
    if service == RC && a.args.get("Channel").is_some_and(|v| v != "Master") {
        return Err(Fault(402, "Invalid Args"));
    }
    let out = match (service, a.name.as_str()) {
        (AV, "SetAVTransportURI") => {
            let uri = a.arg("CurrentURI")?.to_string();
            if !uri.is_empty() {
                let url =
                    reqwest::Url::parse(&uri).map_err(|_| Fault(716, "Resource not found"))?;
                if !["http", "https"].contains(&url.scheme()) {
                    return Err(Fault(716, "Resource not found"));
                }
            }
            r.command(Command::Load {
                uri,
                metadata: a.arg("CurrentURIMetaData")?.into(),
            })
            .await?;
            String::new()
        }
        (AV, "Play") => {
            if a.arg("Speed")? != "1" {
                return Err(Fault(717, "Play speed not supported"));
            }
            if s.uri.is_empty() {
                return Err(Fault(701, "Transition not available"));
            }
            r.command(Command::Play).await?;
            String::new()
        }
        (AV, "Pause") => {
            r.command(Command::Pause).await?;
            String::new()
        }
        (AV, "Stop") => {
            r.command(Command::Stop).await?;
            String::new()
        }
        (AV, "Seek") => {
            if a.arg("Unit")? != "REL_TIME" {
                return Err(Fault(710, "Seek mode not supported"));
            }
            if !s.seekable {
                return Err(Fault(710, "Seek mode not supported"));
            }
            r.command(Command::Seek(parse_time(a.arg("Target")?)?))
                .await?;
            String::new()
        }
        (AV, "GetTransportInfo") => {
            field("CurrentTransportState", s.state)
                + &field(
                    "CurrentTransportStatus",
                    if s.error { "ERROR_OCCURRED" } else { "OK" },
                )
                + &field("CurrentSpeed", 1)
        }
        (AV, "GetPositionInfo") => {
            field("Track", if s.uri.is_empty() { 0 } else { 1 })
                + &field("TrackDuration", time(s.duration))
                + &field("TrackMetaData", &s.metadata)
                + &field("TrackURI", &s.uri)
                + &field("RelTime", time(s.position))
                + &field("AbsTime", time(s.position))
                + &field("RelCount", -1)
                + &field("AbsCount", -1)
        }
        (AV, "GetMediaInfo") => {
            field("NrTracks", if s.uri.is_empty() { 0 } else { 1 })
                + &field("MediaDuration", time(s.duration))
                + &field("CurrentURI", &s.uri)
                + &field("CurrentURIMetaData", &s.metadata)
                + &field("NextURI", "")
                + &field("NextURIMetaData", "")
                + &field(
                    "PlayMedium",
                    if s.uri.is_empty() { "NONE" } else { "NETWORK" },
                )
                + &field("RecordMedium", "NOT_IMPLEMENTED")
                + &field("WriteStatus", "NOT_IMPLEMENTED")
        }
        (AV, "GetDeviceCapabilities") => {
            field("PlayMedia", "NETWORK")
                + &field("RecMedia", "NOT_IMPLEMENTED")
                + &field("RecQualityModes", "NOT_IMPLEMENTED")
        }
        (AV, "GetTransportSettings") => {
            field("PlayMode", "NORMAL") + &field("RecQualityMode", "NOT_IMPLEMENTED")
        }
        (AV, "GetCurrentTransportActions") => field(
            "Actions",
            if s.seekable {
                "Play,Pause,Stop,Seek"
            } else {
                "Play,Pause,Stop"
            },
        ),
        (RC, "GetVolume") => field("CurrentVolume", s.volume),
        (RC, "SetVolume") => {
            let volume = a
                .arg("DesiredVolume")?
                .parse::<u16>()
                .map_err(|_| Fault(402, "Invalid Args"))?;
            if volume > 100 {
                return Err(Fault(402, "Invalid Args"));
            }
            r.command(Command::Volume(volume)).await?;
            String::new()
        }
        (RC, "GetMute") => field("CurrentMute", u8::from(s.mute)),
        (RC, "SetMute") => {
            let mute = match a.arg("DesiredMute")? {
                "1" | "true" => true,
                "0" | "false" => false,
                _ => return Err(Fault(402, "Invalid Args")),
            };
            r.command(Command::Mute(mute)).await?;
            String::new()
        }
        (RC, "ListPresets") => field("CurrentPresetNameList", "FactoryDefaults"),
        (CM, "GetProtocolInfo") => field("Source", "") + &field("Sink", SINK),
        (CM, "GetCurrentConnectionIDs") => field("ConnectionIDs", "0"),
        (CM, "GetCurrentConnectionInfo") => {
            if a.arg("ConnectionID")? != "0" {
                return Err(Fault(706, "Invalid connection reference"));
            }
            field("RcsID", 0)
                + &field("AVTransportID", 0)
                + &field("ProtocolInfo", "")
                + &field("PeerConnectionManager", "")
                + &field("PeerConnectionID", -1)
                + &field("Direction", "Input")
                + &field("Status", "OK")
        }
        _ => return Err(Fault(401, "Invalid Action")),
    };
    Ok(out)
}
async fn subscription(
    State(r): State<Arc<Receiver>>,
    Path(service): Path<String>,
    method: Method,
    headers: HeaderMap,
) -> Response {
    if ![AV, RC, CM].contains(&service.as_str()) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let header = |name: &str| headers.get(name).and_then(|v| v.to_str().ok());
    let sid = header("SID");
    if method.as_str() == "UNSUBSCRIBE" {
        if header("CALLBACK").is_some() || header("NT").is_some() {
            return StatusCode::BAD_REQUEST.into_response();
        }
        let mut subscriptions = r.subscriptions.lock().await;
        if sid.is_some_and(|sid| subscriptions.get(sid).is_some_and(|s| s.service == service)) {
            subscriptions.remove(sid.unwrap());
            return StatusCode::OK.into_response();
        }
        return StatusCode::PRECONDITION_FAILED.into_response();
    }
    if method.as_str() != "SUBSCRIBE" {
        return StatusCode::METHOD_NOT_ALLOWED.into_response();
    }
    let timeout = header("TIMEOUT")
        .and_then(|v| v.strip_prefix("Second-"))
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(1800)
        .clamp(30, 1800);
    let id = if let Some(sid) = sid {
        if header("CALLBACK").is_some() || header("NT").is_some() {
            return StatusCode::BAD_REQUEST.into_response();
        }
        let mut subs = r.subscriptions.lock().await;
        match subs.get_mut(sid) {
            Some(s) if s.service == service && s.expires > Instant::now() => {
                s.expires = Instant::now() + Duration::from_secs(timeout);
                sid.to_string()
            }
            _ => return StatusCode::PRECONDITION_FAILED.into_response(),
        }
    } else {
        if header("NT") != Some("upnp:event") {
            return StatusCode::PRECONDITION_FAILED.into_response();
        }
        let Some(callback) = header("CALLBACK")
            .and_then(|v| v.strip_prefix('<'))
            .and_then(|v| v.strip_suffix('>'))
        else {
            return StatusCode::PRECONDITION_FAILED.into_response();
        };
        let Ok(url) = reqwest::Url::parse(callback) else {
            return StatusCode::PRECONDITION_FAILED.into_response();
        };
        // Callbacks must remain on the LAN. Do not use the receiver as an arbitrary URL fetcher.
        if url.scheme() != "http"
            || !url.username().is_empty()
            || url.password().is_some()
            || !url
                .host_str()
                .and_then(|h| h.parse::<std::net::Ipv4Addr>().ok())
                .is_some_and(|ip| ip.is_private() || ip.is_loopback() || ip.is_link_local())
        {
            return StatusCode::PRECONDITION_FAILED.into_response();
        }
        let id = format!("uuid:{}", uuid::Uuid::new_v4());
        {
            let mut subs = r.subscriptions.lock().await;
            subs.retain(|_, s| s.expires > Instant::now());
            if subs.len() >= 64 {
                return StatusCode::SERVICE_UNAVAILABLE.into_response();
            }
            subs.insert(
                id.clone(),
                Subscription {
                    service,
                    callback: callback.into(),
                    expires: Instant::now() + Duration::from_secs(timeout),
                    sequence: 0,
                    failures: 0,
                },
            );
        }
        let notify = r.clone();
        let initial = id.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            notify.notify(Some(&initial)).await;
        });
        id
    };
    ([("SID", id), ("TIMEOUT", format!("Second-{timeout}"))]).into_response()
}
fn event_body(service: &str, s: &Snapshot) -> String {
    let properties = if service == CM {
        field("SourceProtocolInfo", "")
            + &field("SinkProtocolInfo", SINK)
            + &field("CurrentConnectionIDs", 0)
    } else {
        let attributes = if service == AV {
            format!(
                "<TransportState val=\"{}\"/><TransportStatus val=\"{}\"/><CurrentTrackURI val=\"{}\"/><AVTransportURI val=\"{}\"/><CurrentTrackDuration val=\"{}\"/><RelativeTimePosition val=\"{}\"/>",
                s.state,
                if s.error { "ERROR_OCCURRED" } else { "OK" },
                escape(&s.uri),
                escape(&s.uri),
                time(s.duration),
                time(s.position)
            )
        } else {
            format!(
                "<Volume channel=\"Master\" val=\"{}\"/><Mute channel=\"Master\" val=\"{}\"/>",
                s.volume,
                u8::from(s.mute)
            )
        };
        field(
            "LastChange",
            format!(
                "<Event xmlns=\"urn:schemas-upnp-org:metadata-1-0/{}/\"><InstanceID val=\"0\">{attributes}</InstanceID></Event>",
                if service == AV { "AVT" } else { "RCS" }
            ),
        )
    };
    format!(
        "<e:propertyset xmlns:e=\"urn:schemas-upnp-org:event-1-0\">{} </e:propertyset>",
        if service == CM {
            [
                field("SourceProtocolInfo", ""),
                field("SinkProtocolInfo", SINK),
                field("CurrentConnectionIDs", 0),
            ]
            .map(|p| format!("<e:property>{p}</e:property>"))
            .join("")
        } else {
            format!("<e:property>{properties}</e:property>")
        }
    )
}
// Declarative service schemas keep advertised actions aligned with actual handlers.
// Each argument is name:direction:state-variable. State variable types are below.
fn schemas(service: &str) -> Vec<(&'static str, &'static str)> {
    match service {
        AV => vec![
            (
                "SetAVTransportURI",
                "InstanceID:in:A_ARG_TYPE_InstanceID CurrentURI:in:AVTransportURI CurrentURIMetaData:in:AVTransportURIMetaData",
            ),
            (
                "Play",
                "InstanceID:in:A_ARG_TYPE_InstanceID Speed:in:TransportPlaySpeed",
            ),
            ("Pause", "InstanceID:in:A_ARG_TYPE_InstanceID"),
            ("Stop", "InstanceID:in:A_ARG_TYPE_InstanceID"),
            (
                "Seek",
                "InstanceID:in:A_ARG_TYPE_InstanceID Unit:in:A_ARG_TYPE_SeekMode Target:in:A_ARG_TYPE_SeekTarget",
            ),
            (
                "GetTransportInfo",
                "InstanceID:in:A_ARG_TYPE_InstanceID CurrentTransportState:out:TransportState CurrentTransportStatus:out:TransportStatus CurrentSpeed:out:TransportPlaySpeed",
            ),
            (
                "GetPositionInfo",
                "InstanceID:in:A_ARG_TYPE_InstanceID Track:out:CurrentTrack TrackDuration:out:CurrentTrackDuration TrackMetaData:out:CurrentTrackMetaData TrackURI:out:CurrentTrackURI RelTime:out:RelativeTimePosition AbsTime:out:AbsoluteTimePosition RelCount:out:RelativeCounterPosition AbsCount:out:AbsoluteCounterPosition",
            ),
            (
                "GetMediaInfo",
                "InstanceID:in:A_ARG_TYPE_InstanceID NrTracks:out:NumberOfTracks MediaDuration:out:CurrentMediaDuration CurrentURI:out:AVTransportURI CurrentURIMetaData:out:AVTransportURIMetaData NextURI:out:NextAVTransportURI NextURIMetaData:out:NextAVTransportURIMetaData PlayMedium:out:PlaybackStorageMedium RecordMedium:out:RecordStorageMedium WriteStatus:out:RecordMediumWriteStatus",
            ),
            (
                "GetDeviceCapabilities",
                "InstanceID:in:A_ARG_TYPE_InstanceID PlayMedia:out:PossiblePlaybackStorageMedia RecMedia:out:PossibleRecordStorageMedia RecQualityModes:out:PossibleRecordQualityModes",
            ),
            (
                "GetTransportSettings",
                "InstanceID:in:A_ARG_TYPE_InstanceID PlayMode:out:CurrentPlayMode RecQualityMode:out:CurrentRecordQualityMode",
            ),
            (
                "GetCurrentTransportActions",
                "InstanceID:in:A_ARG_TYPE_InstanceID Actions:out:CurrentTransportActions",
            ),
        ],
        RC => vec![
            (
                "ListPresets",
                "InstanceID:in:A_ARG_TYPE_InstanceID CurrentPresetNameList:out:PresetNameList",
            ),
            (
                "GetVolume",
                "InstanceID:in:A_ARG_TYPE_InstanceID Channel:in:A_ARG_TYPE_Channel CurrentVolume:out:Volume",
            ),
            (
                "SetVolume",
                "InstanceID:in:A_ARG_TYPE_InstanceID Channel:in:A_ARG_TYPE_Channel DesiredVolume:in:Volume",
            ),
            (
                "GetMute",
                "InstanceID:in:A_ARG_TYPE_InstanceID Channel:in:A_ARG_TYPE_Channel CurrentMute:out:Mute",
            ),
            (
                "SetMute",
                "InstanceID:in:A_ARG_TYPE_InstanceID Channel:in:A_ARG_TYPE_Channel DesiredMute:in:Mute",
            ),
        ],
        CM => vec![
            (
                "GetProtocolInfo",
                "Source:out:SourceProtocolInfo Sink:out:SinkProtocolInfo",
            ),
            (
                "GetCurrentConnectionIDs",
                "ConnectionIDs:out:CurrentConnectionIDs",
            ),
            (
                "GetCurrentConnectionInfo",
                "ConnectionID:in:A_ARG_TYPE_ConnectionID RcsID:out:A_ARG_TYPE_RcsID AVTransportID:out:A_ARG_TYPE_AVTransportID ProtocolInfo:out:A_ARG_TYPE_ProtocolInfo PeerConnectionManager:out:A_ARG_TYPE_ConnectionManager PeerConnectionID:out:A_ARG_TYPE_ConnectionID Direction:out:A_ARG_TYPE_Direction Status:out:A_ARG_TYPE_ConnectionStatus",
            ),
        ],
        _ => vec![],
    }
}
async fn service_description(Path(service): Path<String>) -> Response {
    if ![AV, RC, CM].contains(&service.as_str()) {
        return StatusCode::NOT_FOUND.into_response();
    }
    xml(scpd(&service))
}
fn scpd(service: &str) -> String {
    let mut variables = std::collections::BTreeSet::new();
    let actions=schemas(service).into_iter().map(|(action,arguments)| {
        let args=arguments.split_whitespace().map(|argument| {
            let parts:Vec<_>=argument.split(':').collect();variables.insert(parts[2]);
            format!("<argument><name>{}</name><direction>{}</direction><relatedStateVariable>{}</relatedStateVariable></argument>",parts[0],parts[1],parts[2])
        }).collect::<String>();
        format!("<action><name>{action}</name><argumentList>{args}</argumentList></action>")
    }).collect::<String>();
    if service != CM {
        variables.insert("LastChange");
    }
    let states=variables.into_iter().map(|name| {
        let data_type=match name {"A_ARG_TYPE_InstanceID"|"CurrentTrack"|"NumberOfTracks"=>"ui4","Volume"=>"ui2","Mute"=>"boolean","RelativeCounterPosition"|"AbsoluteCounterPosition"|"A_ARG_TYPE_ConnectionID"|"A_ARG_TYPE_RcsID"|"A_ARG_TYPE_AVTransportID"=>"i4",_=>"string"};
        let values=match name {
            "TransportState"=>&["STOPPED","PLAYING","TRANSITIONING","PAUSED_PLAYBACK","NO_MEDIA_PRESENT"][..],
            "TransportStatus"=>&["OK","ERROR_OCCURRED"][..],"TransportPlaySpeed"=>&["1"][..],
            "A_ARG_TYPE_SeekMode"=>&["REL_TIME"][..],"A_ARG_TYPE_Channel"=>&["Master"][..],
            "CurrentPlayMode"=>&["NORMAL"][..],"A_ARG_TYPE_Direction"=>&["Input"][..],
            "A_ARG_TYPE_ConnectionStatus"=>&["OK","ContentFormatMismatch","InsufficientBandwidth","UnreliableChannel","Unknown"][..],_=>&[][..],
        };
        let allowed=if name=="Volume" {"<allowedValueRange><minimum>0</minimum><maximum>100</maximum><step>1</step></allowedValueRange>".into()} else if values.is_empty(){String::new()} else {format!("<allowedValueList>{}</allowedValueList>",values.iter().map(|v|field("allowedValue",v)).collect::<String>())};
        let event=if name=="LastChange" || (service==CM && ["SourceProtocolInfo","SinkProtocolInfo","CurrentConnectionIDs"].contains(&name)){"yes"}else{"no"};
        format!("<stateVariable sendEvents=\"{event}\"><name>{name}</name><dataType>{data_type}</dataType>{allowed}</stateVariable>")
    }).collect::<String>();
    format!(
        "<?xml version=\"1.0\"?><scpd xmlns=\"urn:schemas-upnp-org:service-1-0\"><specVersion><major>1</major><minor>0</minor></specVersion><actionList>{actions}</actionList><serviceStateTable>{states}</serviceStateTable></scpd>"
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn soap_handles_arbitrary_namespace_prefix_and_escaped_uri() {
        let action=Action::parse("<soap:Envelope xmlns:soap='http://schemas.xmlsoap.org/soap/envelope/'><soap:Body><av:SetAVTransportURI xmlns:av='urn:schemas-upnp-org:service:AVTransport:1'><InstanceID>0</InstanceID><CurrentURI>http://example.com/a?x=1&amp;y=2</CurrentURI><CurrentURIMetaData/></av:SetAVTransportURI></soap:Body></soap:Envelope>",AV).unwrap();
        assert_eq!(
            action.arg("CurrentURI").unwrap(),
            "http://example.com/a?x=1&y=2"
        );
        assert!(Action::parse("<Envelope><Play/></Envelope>", AV).is_err());
    }
    #[test]
    fn seek_rejects_invalid_or_nonfinite_targets() {
        assert_eq!(parse_time("01:02:03.5").unwrap(), 3723.5);
        for value in ["12", "00:60:00", "00:00:NaN", "00:00:-1"] {
            assert!(parse_time(value).is_err());
        }
    }
    #[test]
    fn service_schemas_and_events_are_valid_xml() {
        for service in [AV, RC, CM] {
            roxmltree::Document::parse(&scpd(service)).unwrap();
            roxmltree::Document::parse(&event_body(service, &Snapshot::default())).unwrap();
        }
        let s = Snapshot {
            uri: "http://example.com/?a=1&b=2".into(),
            ..Snapshot::default()
        };
        let xml = event_body(AV, &s);
        let doc = roxmltree::Document::parse(&xml).unwrap();
        let last = doc
            .descendants()
            .find(|n| n.has_tag_name("LastChange"))
            .unwrap()
            .text()
            .unwrap();
        let event = roxmltree::Document::parse(last).unwrap();
        assert_eq!(
            event
                .descendants()
                .find(|n| n.has_tag_name("AVTransportURI"))
                .unwrap()
                .attribute("val"),
            Some(s.uri.as_str())
        );
    }
    #[tokio::test]
    async fn http_routes_validate_actions_without_touching_player() {
        let (commands, mut requests) = mpsc::channel(1);
        let (_snapshot, states) = watch::channel(Snapshot::default());
        let receiver = Arc::new(Receiver::new(
            commands,
            states,
            "Test & Receiver".into(),
            uuid::Uuid::new_v4(),
        ));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let (shutdown, signal) = watch::channel(false);
        let server = tokio::spawn(serve(listener, receiver, signal));
        let client = reqwest::Client::new();
        let response = client
            .get(format!("{base}/description.xml"))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let description = response.text().await.unwrap();
        let document = roxmltree::Document::parse(&description).unwrap();
        assert_eq!(
            document
                .descendants()
                .find(|n| n.tag_name().name() == "friendlyName")
                .unwrap()
                .text(),
            Some("Test & Receiver")
        );
        for service in [AV, RC, CM] {
            let response = client
                .get(format!("{base}/service/{service}"))
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            roxmltree::Document::parse(&response.text().await.unwrap()).unwrap();
        }
        for (service, action, arguments, code) in [
            (
                AV,
                "Play",
                "<InstanceID>0</InstanceID><Speed>1</Speed>",
                701,
            ),
            (
                RC,
                "SetVolume",
                "<InstanceID>0</InstanceID><DesiredVolume>50</DesiredVolume>",
                402,
            ),
            (
                RC,
                "SetVolume",
                "<InstanceID>0</InstanceID><Channel>Master</Channel><DesiredVolume>101</DesiredVolume>",
                402,
            ),
            (
                AV,
                "SetAVTransportURI",
                "<InstanceID>0</InstanceID><CurrentURI>file:///private/test.mp4</CurrentURI><CurrentURIMetaData/>",
                716,
            ),
            (AV, "GetTransportInfo", "<InstanceID>1</InstanceID>", 718),
        ] {
            let body = format!(
                "<s:Envelope xmlns:s='http://schemas.xmlsoap.org/soap/envelope/'><s:Body><u:{action} xmlns:u='urn:schemas-upnp-org:service:{service}:1'>{arguments}</u:{action}></s:Body></s:Envelope>"
            );
            let response = client
                .post(format!("{base}/control/{service}"))
                .header(
                    "SOAPAction",
                    format!("urn:schemas-upnp-org:service:{service}:1#{action}"),
                )
                .body(body)
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
            assert!(
                response
                    .text()
                    .await
                    .unwrap()
                    .contains(&format!("<errorCode>{code}</errorCode>"))
            );
        }
        assert!(requests.try_recv().is_err());
        shutdown.send(true).unwrap();
        server.await.unwrap().unwrap();
    }
}

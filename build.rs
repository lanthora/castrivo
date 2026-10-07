fn main() {
    pkg_config::Config::new()
        .atleast_version("2.0")
        .probe("mpv")
        .expect("Install libmpv development libraries and expose mpv.pc through PKG_CONFIG_PATH");
}

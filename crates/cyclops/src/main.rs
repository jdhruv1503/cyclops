fn main() {
    let config = cyclops::config::Config::parse();
    std::process::exit(cyclops::run(config));
}

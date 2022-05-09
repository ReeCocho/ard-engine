use chrono::{Datelike, Timelike};
// Re-export logging functions for convenience.
pub use log::*;
use log4rs::{
    append::{console::ConsoleAppender, file::FileAppender},
    config::{Appender, Logger, Root},
    encode::pattern::PatternEncoder,
    Config,
};

/// Initializes logging. Should be called before any other logging functions. Provided
/// `LevelFilter` will remove all logs below the provided level.
pub fn init(filter: LevelFilter) {
    // Output to console
    let stdout = ConsoleAppender::builder().build();

    // Output to log file. Name of the file is based on the current time.
    let now = chrono::Utc::now();
    let log_file = FileAppender::builder()
        .encoder(Box::new(PatternEncoder::new("{d} - {m}{n}")))
        .build(format!(
            "./logs/{} {} {} {} {} {}.txt",
            now.year(),
            now.month(),
            now.date(),
            now.hour(),
            now.minute(),
            now.second()
        ))
        .expect("unable to initialize logging to file");

    let config = Config::builder()
        .appender(Appender::builder().build("stdout", Box::new(stdout)))
        .appender(Appender::builder().build("log_file", Box::new(log_file)))
        .logger(
            Logger::builder()
                .appender("log_file")
                .additive(false)
                .build("app::log_file", filter),
        )
        .build(
            Root::builder()
                .appender("log_file")
                .appender("stdout")
                .build(filter),
        )
        .expect("unable to create logging configuration");

    log4rs::init_config(config).expect("unable to initialize logging");

    log_panics::init();
}

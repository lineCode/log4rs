//! An encoder which writes a JSON object.
//!
//! Each log event will be written as a JSON object on its own line.
//!
//! Requires the `json_encoder` feature.
//!
//! # Contents
//!
//! An example object (note that real output will not be pretty-printed):
//!
//! ```json
//! {
//!     "time": "2016-03-20T14:22:20.644420340-08:00",
//!     "message": "the log message",
//!     "module_path": "foo::bar",
//!     "file": "foo/bar/mod.rs",
//!     "line": 100,
//!     "level": "INFO",
//!     "target": "foo::bar",
//!     "thread": "main",
//!     "mdc": {
//!         "request_id": "123e4567-e89b-12d3-a456-426655440000"
//!     }
//! }
//! ```

use chrono::{DateTime, Local};
use chrono::format::{DelayedFormat, Item, Fixed};
use log::{LogLevel, LogRecord};
use log_mdc;
use std::error::Error;
use std::fmt;
use std::thread;
use std::option;
use serde::ser::{self, Serialize, SerializeMap};
use serde_json;

use encode::{Encode, Write, NEWLINE};
#[cfg(feature = "file")]
use file::{Deserialize, Deserializers};

/// The JSON encoder's configuration
#[cfg(feature = "file")]
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JsonEncoderConfig {
    #[serde(skip_deserializing)]
    _p: (),
}

/// An `Encode`r which writes a JSON object.
#[derive(Debug)]
pub struct JsonEncoder(());

impl JsonEncoder {
    /// Returns a new `JsonEncoder` with a default configuration.
    pub fn new() -> JsonEncoder {
        JsonEncoder(())
    }
}

impl JsonEncoder {
    fn encode_inner(&self,
                    w: &mut Write,
                    time: DateTime<Local>,
                    level: LogLevel,
                    target: &str,
                    module_path: &str,
                    file: &str,
                    line: u32,
                    args: &fmt::Arguments)
                    -> Result<(), Box<Error + Sync + Send>> {
        let thread = thread::current();
        let message = Message {
            time: time.format_with_items(Some(Item::Fixed(Fixed::RFC3339)).into_iter()),
            message: args,
            level: level_str(level),
            module_path: module_path,
            file: file,
            line: line,
            target: target,
            thread: thread.name(),
            mdc: Mdc,
        };
        message.serialize(&mut serde_json::Serializer::new(&mut *w))?;
        w.write_all(NEWLINE.as_bytes())?;
        Ok(())
    }
}

impl Encode for JsonEncoder {
    fn encode(&self, w: &mut Write, record: &LogRecord) -> Result<(), Box<Error + Sync + Send>> {
        self.encode_inner(w,
                          Local::now(),
                          record.level(),
                          record.target(),
                          record.location().module_path(),
                          record.location().file(),
                          record.location().line(),
                          record.args())
    }
}

#[derive(Serialize)]
struct Message<'a> {
    #[serde(serialize_with = "ser_display")]
    time: DelayedFormat<option::IntoIter<Item<'a>>>,
    #[serde(serialize_with = "ser_display")]
    message: &'a fmt::Arguments<'a>,
    module_path: &'a str,
    file: &'a str,
    line: u32,
    level: &'static str,
    target: &'a str,
    thread: Option<&'a str>,
    mdc: Mdc,
}

fn level_str(level: LogLevel) -> &'static str {
    match level {
        LogLevel::Error => "ERROR",
        LogLevel::Warn => "WARN",
        LogLevel::Info => "INFO",
        LogLevel::Debug => "DEBUG",
        LogLevel::Trace => "TRACE",
    }
}

fn ser_display<T, S>(v: &T, s: S) -> Result<S::Ok, S::Error>
    where T: fmt::Display,
          S: ser::Serializer
{
    s.collect_str(v)
}

struct Mdc;

impl ser::Serialize for Mdc {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where S: ser::Serializer
    {
        let mut map = serializer.serialize_map(None)?;

        let mut err = Ok(());
        log_mdc::iter(|k, v| {
            if let Ok(()) = err {
                err = map.serialize_key(k)
                    .and_then(|()| map.serialize_value(v));
            }
        });
        err?;

        map.end()
    }
}

/// A deserializer for the `JsonEncoder`.
///
/// # Configuration
///
/// ```yaml
/// kind: json
/// ```
#[cfg(feature = "file")]
pub struct JsonEncoderDeserializer;

#[cfg(feature = "file")]
impl Deserialize for JsonEncoderDeserializer {
    type Trait = Encode;

    type Config = JsonEncoderConfig;

    fn deserialize(&self,
                   _: JsonEncoderConfig,
                   _: &Deserializers)
                   -> Result<Box<Encode>, Box<Error + Sync + Send>> {
        Ok(Box::new(JsonEncoder::new()))
    }
}

#[cfg(test)]
#[cfg(feature = "simple_writer")]
mod test {
    use chrono::{DateTime, Local};
    use log::LogLevel;
    use log_mdc;

    use encode::writer::simple::SimpleWriter;
    use super::*;

    #[test]
    fn default() {
        let time = DateTime::parse_from_rfc3339("2016-03-20T14:22:20.644420340-08:00")
            .unwrap()
            .with_timezone(&Local);
        let level = LogLevel::Debug;
        let target = "target";
        let module_path = "module_path";
        let file = "file";
        let line = 100;
        let message = "message";
        let thread = "encode::json::test::default";
        log_mdc::insert("foo", "bar");

        let encoder = JsonEncoder::new();

        let mut buf = vec![];
        encoder.encode_inner(&mut SimpleWriter(&mut buf),
                          time,
                          level,
                          target,
                          module_path,
                          file,
                          line,
                          &format_args!("{}", message))
            .unwrap();

        let expected = format!("{{\"time\":\"{}\",\"message\":\"{}\",\"module_path\":\"{}\",\
                                \"file\":\"{}\",\"line\":{},\"level\":\"{}\",\"target\":\"{}\",\
                                \"thread\":\"{}\",\"mdc\":{{\"foo\":\"bar\"}}}}\n",
                               time.to_rfc3339(),
                               message,
                               module_path,
                               file,
                               line,
                               level,
                               target,
                               thread);
        assert_eq!(expected, String::from_utf8(buf).unwrap());
    }
}

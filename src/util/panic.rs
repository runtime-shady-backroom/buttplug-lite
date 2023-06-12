// This file is heavily inspired by https://github.com/rust-cli/human-panic
// human-panic is licensed under MIT OR Apache-2.0
// Copyright 2018 human-panic Individual contributors
// Copyright 2023 runtime-shady-backroom

/// Handles custom panic hook and logging

use std::{mem, panic, thread};
use std::fmt::Write as FmtWrite;

use backtrace::{Backtrace, BacktraceFrame};
use tracing::error;

// We take padding for address and extra two letters to pad after index.
const HEX_WIDTH: usize = mem::size_of::<usize>() + 2;
// Padding for next lines after frame's address
const NEXT_SYMBOL_PADDING: usize = HEX_WIDTH + 6;

pub fn set_hook() {
    panic::set_hook(Box::new(|panic_info| {
        let payload = panic_info.payload();
        let cause = match (
            payload.downcast_ref::<&str>(),
            payload.downcast_ref::<String>(),
        ) {
            (_, Some(s)) => Some(s.to_string()),
            (Some(s), _) => Some(s.to_string()),
            (_, _) => None,
        };

        let cause = match cause {
            Some(cause) => cause,
            None => "<unknown>".to_string(),
        };

        let location = match panic_info.location() {
            Some(location) => format!("{}:{}:{}", location.file(), location.line(), location.column()),
            None => "<unknown>".to_string(),
        };

        let mut backtrace = String::new();

        for (index, frame) in Backtrace::new()
            .frames()
            .iter()
            .skip_while(should_skip) // skip until we see the stack frame where panic internals start
            .enumerate()
        {
            let ip = frame.ip();
            let _ = write!(backtrace, "\n{index:4}: {ip:HEX_WIDTH$?}");

            match frame.symbols() {
                &[] => continue,
                symbols => {
                    for (index, symbol) in symbols.iter().enumerate() {
                        // Print symbols from this address.
                        // If there are several addresses we need to put it on the next line.
                        if index != 0 {
                            let _ = write!(backtrace, "\n{:1$}", "", NEXT_SYMBOL_PADDING);
                        }

                        if let Some(name) = symbol.name() {
                            let _ = write!(backtrace, " - {name}");
                        } else {
                            let _ = write!(backtrace, " - <unknown>");
                        }

                        // See if there is debug information with file name and line
                        if let (Some(file), Some(line)) = (symbol.filename(), symbol.lineno()) {
                            let _ = write!(
                                backtrace,
                                "\n{:3$}at {}:{}",
                                "",
                                file.display(),
                                line,
                                NEXT_SYMBOL_PADDING
                            );
                        }
                    }
                }
            }
        }

        // A typical one-liner panic looks like this:
        // thread 'util::panic::tests::normal_panic' panicked at 'normal_panic', src\util\panic.rs:31:9
        // we'll emulate that format for our first line, but also add a backtrace
        let thread_name = thread::current().name().map_or_else(|| "<unknown>", |s| s).to_string();
        error!("{} v{} has crashed.\nTo help me diagnose this problem you can attach this log file to a new GitHub issue at https://github.com/runtime-shady-backroom/buttplug-lite/issues\nthread '{thread_name}' panicked at '{cause}', {location}{backtrace}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
    }));
}

/// Should this stack frame be skipped?
fn should_skip(frame: &&BacktraceFrame) -> bool {
    match frame.symbols() {
        [first, ..] => {
            if let Some(name) = first.name() {
                let name = format!("{name}");
                name != "std::panicking::begin_panic_handler" && name != "core::panicking::panic_fmt" && name != "core::panicking::panic"
            } else {
                false
            }
        }
        _ => false
    }
}

#[cfg(test)]
mod tests {
    use crate::util::logging;

    use super::*;

    #[test]
    #[should_panic]
    fn normal_panic() {
        panic!("normal_panic");
    }

    #[test]
    #[should_panic]
    fn hooked_panic() {
        logging::init_console(true);
        set_hook();
        panic!("hooked_panic");

        // panics after core::panicking::panic_fmt
    }

    #[test]
    #[should_panic]
    fn hooked_unwrap() {
        logging::init_console(true);
        set_hook();
        let empty: Option<&str> = None;
        empty.unwrap();

        // panics after core::panicking::panic_fmt
    }

    #[test]
    #[should_panic]
    fn hooked_empty_panic() {
        logging::init_console(true);
        set_hook();
        panic!();
    }

    #[test]
    fn str_works() {
        set_hook();

        let result = panic::catch_unwind(|| {
            panic!("str_works");
        });

        assert!(result.is_err());

        let cause = result.unwrap_err();
        let str = cause.downcast_ref::<&str>();
        assert!(str.is_some());
    }

    #[test]
    fn string_works() {
        set_hook();

        let result = panic::catch_unwind(|| {
            let string = "string_works".to_string();
            panic!("{}", string);
        });

        assert!(result.is_err());

        let cause = result.unwrap_err();
        let str = cause.downcast_ref::<String>();
        assert!(str.is_some());
    }

    #[test]
    fn string_to_string_works() {
        set_hook();

        let result = panic::catch_unwind(|| {
            let string = "string_to_string_works".to_string();
            panic!("{}", string);
        });

        assert!(result.is_err());

        let cause = result.unwrap_err();
        let string = match (
            cause.downcast_ref::<&str>(),
            cause.downcast_ref::<String>(),
        ) {
            (_, Some(s)) => Some(s.to_string()),
            (Some(s), _) => Some(s.to_string()),
            (_, _) => None,
        };
        assert!(string.is_some());
    }

    #[test]
    fn str_to_string_works() {
        set_hook();

        let result = panic::catch_unwind(|| {
            panic!("str_to_string_works");
        });

        assert!(result.is_err());

        let cause = result.unwrap_err();
        let string = match (
            cause.downcast_ref::<&str>(),
            cause.downcast_ref::<String>(),
        ) {
            (_, Some(s)) => Some(s.to_string()),
            (Some(s), _) => Some(s.to_string()),
            (_, _) => None,
        };
        assert!(string.is_some());
    }
}

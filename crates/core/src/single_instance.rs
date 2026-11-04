//! One running instance per user session. A second launch forwards its argv
//! to the live window over a local socket instead of opening a second store
//! and engine set.

use std::io::{BufRead, BufReader, Read, Write};

use futures::channel::mpsc::{self, Receiver};
use interprocess::local_socket::prelude::*;
use interprocess::local_socket::{GenericNamespaced, ListenerOptions, Stream};

const PIPE_NAME: &str = "manyproxy.instance";
/// A malformed client must not grow the listener's line buffer without bound.
const MAX_ARGV_LINE: u64 = 64 * 1024;
/// Relaunches are user-speed, so a tiny queue is enough; a full one drops the
/// forward rather than stalling the listener thread behind a busy UI.
const INSTANCE_QUEUE: usize = 8;

fn pipe_name() -> std::io::Result<interprocess::local_socket::Name<'static>> {
    PIPE_NAME.to_ns_name::<GenericNamespaced>()
}

pub fn encode_argv(args: &[String]) -> Vec<u8> {
    let mut line = serde_json::to_vec(args).unwrap_or_default();
    line.push(b'\n');
    line
}

pub fn decode_argv(line: &[u8]) -> Option<Vec<String>> {
    serde_json::from_slice(line).ok()
}

/// Hand `args` to the running instance. `false` means no listener answered, so
/// the caller should become the first instance.
pub fn forward_to_running(args: &[String]) -> bool {
    let Ok(name) = pipe_name() else {
        return false;
    };
    let Ok(mut stream) = Stream::connect(name) else {
        return false;
    };
    stream.write_all(&encode_argv(args)).is_ok()
}

/// Bind the instance pipe and return a channel of forwarded argv. `None` means
/// another instance owns the pipe (or it could not be created): the caller
/// keeps starting normally rather than refusing to run.
///
/// The channel is async so the UI task can `await` the next launch instead of
/// polling for it; the listener itself stays on a blocking thread because
/// `interprocess`'s sync listener is what owns the pipe.
pub fn listen() -> Option<Receiver<Vec<String>>> {
    let name = pipe_name().ok()?;
    let listener = ListenerOptions::new().name(name).create_sync().ok()?;
    let (mut tx, rx) = mpsc::channel(INSTANCE_QUEUE);
    std::thread::Builder::new()
        .name("manyproxy-instance".to_string())
        .spawn(move || {
            for stream in listener.incoming().flatten() {
                let mut line = Vec::new();
                let mut reader = BufReader::new(stream).take(MAX_ARGV_LINE);
                if reader.read_until(b'\n', &mut line).is_err() {
                    continue;
                }
                let Some(argv) = decode_argv(line.trim_ascii_end()) else {
                    continue;
                };
                match tx.try_send(argv) {
                    Ok(()) => {}
                    // A full queue drops this launch, but the listener stays up
                    // for the next one.
                    Err(error) if error.is_full() => {}
                    Err(_) => break,
                }
            }
        })
        .ok()?;
    Some(rx)
}

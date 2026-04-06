// Dispatcher: called by s6-ipcserver for each incoming unix socket connection.
//
// Why this binary exists
// ----------------------
// s6-ipcserver connects a single socket fd to the child's stdin AND stdout.
// That means the child's stderr has nowhere to go — it is silently discarded.
// This binary wraps the child process, capturing stdout and stderr on separate
// pipes, then multiplexes both into a single framed stream on its own stdout
// using Docker's 8-byte header format:
//
//   [stream_type: u8][0x00 0x00 0x00][length: u32 big-endian][payload]
//
// stream_type: 0x01 = stdout, 0x02 = stderr
//
// The host-side reader (call_via_socket in container.rs) parses these frames
// and emits the appropriate LogOutput::StdOut / LogOutput::StdErr variants,
// so callers see stderr as "error" SSE events instead of losing it entirely.
//
// Two reader threads + mpsc channel serialize stdout and stderr chunks without
// any external dependencies. The channel acts as the ordering primitive — there
// is no race between the two streams.

use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::sync::mpsc;

const STDOUT_TAG: u8 = 0x01;
const STDERR_TAG: u8 = 0x02;

fn write_frame(out: &mut impl Write, tag: u8, data: &[u8]) {
    let len = data.len() as u32;
    let header = [tag, 0x00, 0x00, 0x00,
        (len >> 24) as u8,
        (len >> 16) as u8,
        (len >> 8) as u8,
        len as u8,
    ];
    out.write_all(&header).unwrap();
    out.write_all(data).unwrap();
    out.flush().unwrap();
}

fn reader_thread(mut reader: impl Read + Send + 'static, tag: u8, tx: mpsc::Sender<(u8, Vec<u8>)>) {
    let mut buf = [0u8; 4096];
    loop {
        match reader.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                if tx.send((tag, buf[..n].to_vec())).is_err() {
                    break;
                }
            }
        }
    }
}

fn main() {
    // Read the script name from the first line of stdin.
    let mut stdin_bytes = std::io::stdin();
    let mut cmd = String::new();
    let mut byte = [0u8; 1];
    loop {
        match stdin_bytes.read(&mut byte) {
            Ok(0) | Err(_) => std::process::exit(1),
            Ok(_) => {
                if byte[0] == b'\n' {
                    break;
                }
                cmd.push(byte[0] as char);
            }
        }
    }
    let cmd = cmd.trim().to_string();

    // Reject anything that tries to escape /app/bin/.
    if cmd.contains('/') || cmd.contains("..") || cmd.is_empty() {
        std::process::exit(1);
    }

    let mut child = match Command::new(format!("/app/bin/{cmd}"))
        .stdin(Stdio::inherit())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(_) => std::process::exit(127),
    };

    let child_stdout = child.stdout.take().unwrap();
    let child_stderr = child.stderr.take().unwrap();

    let (tx, rx) = mpsc::channel::<(u8, Vec<u8>)>();

    let tx2 = tx.clone();
    let t1 = std::thread::spawn(move || reader_thread(child_stdout, STDOUT_TAG, tx));
    let t2 = std::thread::spawn(move || reader_thread(child_stderr, STDERR_TAG, tx2));

    let mut stdout = std::io::stdout();
    while let Ok((tag, data)) = rx.recv() {
        write_frame(&mut stdout, tag, &data);
    }

    t1.join().unwrap();
    t2.join().unwrap();
    child.wait().unwrap();
}

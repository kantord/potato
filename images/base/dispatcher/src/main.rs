// Dispatcher: called by s6-ipcserver for each incoming unix socket connection.
//
// s6-ipcserver connects a single socket fd to the child's stdin AND stdout,
// so the child's stderr has nowhere to go and is silently discarded.
// This binary wraps the child process, capturing stdout and stderr on separate
// pipes, then multiplexes both into a single framed stream on its own stdout
// using Docker's 8-byte header format:
//
//   [stream_type: u8][0x00 0x00 0x00][length: u32 big-endian][payload]
//
// stream_type: 0x01 = stdout, 0x02 = stderr
//
// The host-side reader (call_via_socket in container.rs) parses these frames
// and emits LogOutput::StdOut / LogOutput::StdErr, so stderr shows up as
// "error" SSE events rather than being dropped.
//
// Two reader threads + mpsc channel handle the multiplexing. The channel
// serializes chunks from both streams so there's no ordering race.

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
    //
    // IMPORTANT: do NOT use std::io::stdin() here. Rust's Stdin wraps fd 0 in
    // a BufReader (8 KB buffer), so the first read() call drains the entire
    // kernel socket buffer into userspace. If the caller sends stdin data
    // before we've spawned the child, that data ends up stuck in our buffer
    // and the child (which inherits the raw fd 0) never sees it, hanging
    // forever. We read directly from fd 0 with no buffering so every byte we
    // consume is exactly one kernel read(), leaving the rest for the child.
    let mut stdin_raw = std::mem::ManuallyDrop::new(unsafe {
        use std::os::fd::FromRawFd;
        std::fs::File::from_raw_fd(0)
    });
    const MAX_CMD_LEN: usize = 255;
    let mut cmd = String::new();
    let mut byte = [0u8; 1];
    loop {
        match stdin_raw.read(&mut byte) {
            Ok(0) | Err(_) => std::process::exit(1),
            Ok(_) => {
                if byte[0] == b'\n' {
                    break;
                }
                if cmd.len() >= MAX_CMD_LEN {
                    std::process::exit(1);
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

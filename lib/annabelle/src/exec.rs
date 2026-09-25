// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Subprocess execution with captured output and a timeout, plus a small
//! bounded worker pool.

use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::sync::{Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

/// Exit code (negative signal number when killed), stdout, stderr.
pub type Output = (i32, String, String);

/// Run `args`, optionally feeding `input` on stdin (otherwise stdin is
/// inherited), and kill it after `timeout`.
pub fn run(args: &[String], input: Option<&str>, timeout: Duration) -> Result<Output, String> {
    let mut child = Command::new(&args[0])
        .args(&args[1..])
        .stdin(match input {
            Some(_) => Stdio::piped(),
            None => Stdio::inherit(),
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;

    let stdin = child.stdin.take();
    let input = input.map(str::to_owned);
    let writer = thread::spawn(move || {
        if let (Some(mut w), Some(data)) = (stdin, input) {
            // A closed pipe just means the remote side stopped reading.
            let _ = w.write_all(data.as_bytes());
        }
    });
    let mut out = child.stdout.take().expect("piped stdout");
    let mut err = child.stderr.take().expect("piped stderr");
    let out_reader = thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = out.read_to_end(&mut buf);
        buf
    });
    let err_reader = thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = err.read_to_end(&mut buf);
        buf
    });

    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait().map_err(|e| e.to_string())? {
            Some(s) => break s,
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "Command {} timed out after {} seconds",
                    py_repr_list(args),
                    timeout.as_secs()
                ));
            }
            None => thread::sleep(Duration::from_millis(20)),
        }
    };
    let _ = writer.join();
    let out = out_reader.join().unwrap_or_default();
    let err = err_reader.join().unwrap_or_default();
    Ok((
        exit_code(status),
        String::from_utf8_lossy(&out).into_owned(),
        String::from_utf8_lossy(&err).into_owned(),
    ))
}

fn exit_code(status: std::process::ExitStatus) -> i32 {
    use std::os::unix::process::ExitStatusExt;
    status
        .code()
        .or_else(|| status.signal().map(|s| -s))
        .unwrap_or(1)
}

/// Python-style repr of a list of strings, e.g. `['a', 'b']`, quoted.
fn py_repr_list(args: &[String]) -> String {
    let items: Vec<String> = args.iter().map(|a| py_repr_str(a)).collect();
    format!("'[{}]'", items.join(", "))
}

fn py_repr_str(s: &str) -> String {
    let q = match s.contains('\'') && !s.contains('"') {
        true => '"',
        false => '\'',
    };
    let mut out = String::from(q);
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c if c == q => {
                out.push('\\');
                out.push(c);
            }
            c => out.push(c),
        }
    }
    out.push(q);
    out
}

/// Run `f` over `tasks` on at most `workers` threads, handing each result
/// to `on_done` on the calling thread in completion order.
pub fn pool<T, R, F, D>(tasks: Vec<T>, workers: usize, f: F, mut on_done: D)
where
    T: Send,
    R: Send,
    F: Fn(T) -> R + Sync,
    D: FnMut(R),
{
    let queue = Mutex::new(tasks.into_iter());
    let (tx, rx) = mpsc::channel();
    thread::scope(|s| {
        for _ in 0..workers.max(1) {
            let tx = tx.clone();
            let (queue, f) = (&queue, &f);
            s.spawn(move || {
                loop {
                    let next = queue.lock().expect("queue lock").next();
                    let Some(task) = next else { break };
                    if tx.send(f(task)).is_err() {
                        break;
                    }
                }
            });
        }
        drop(tx);
        for r in rx {
            on_done(r);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captures_output_and_code() {
        let args: Vec<String> = ["sh", "-c", "cat; echo e >&2; exit 3"]
            .map(String::from)
            .to_vec();
        let (rc, out, err) = run(&args, Some("hi"), Duration::from_secs(10)).unwrap();
        assert_eq!((rc, out.as_str(), err.as_str()), (3, "hi", "e\n"));
    }

    #[test]
    fn times_out() {
        let args: Vec<String> = ["sleep", "5"].map(String::from).to_vec();
        let e = run(&args, Some(""), Duration::from_millis(100)).unwrap_err();
        assert_eq!(e, "Command '['sleep', '5']' timed out after 0 seconds");
    }

    #[test]
    fn pool_runs_all() {
        let mut got = Vec::new();
        pool((0..20).collect(), 3, |x: i32| x * 2, |r| got.push(r));
        got.sort();
        assert_eq!(got, (0..20).map(|x| x * 2).collect::<Vec<_>>());
    }
}

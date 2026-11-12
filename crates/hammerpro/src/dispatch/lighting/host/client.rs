use std::collections::VecDeque;
use std::io::Write;
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, RecvTimeoutError};

use super::engine::Engine;
use super::{Reply, Request, read_frame, write_frame};

type Queue = std::sync::Arc<Mutex<VecDeque<(Reply, Vec<u8>)>>>;

pub(crate) enum Gone {
    Exited,
}

struct Process {
    child: Child,
    input: ChildStdin,
    replies: Receiver<(Reply, Vec<u8>)>,
    started: Instant,
    priority: hp_app::settings::BakePriority,
}

impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

struct Local {
    engine: Engine,
    queue: Queue,
    out: super::engine::Shared,
}

pub(crate) struct Link {
    process: Option<Process>,
    local: Option<Box<Local>>,
}

impl Link {
    pub(crate) fn spawn(exe: &Path, threads: usize) -> std::io::Result<Link> {
        let mut cmd = Command::new(exe);
        cmd.arg(super::super::child::LIGHT_ARG)
            .arg(super::SERVE_ARG)
            .arg(threads.max(1).to_string())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            use windows_sys::Win32::System::Threading::{BELOW_NORMAL_PRIORITY_CLASS, CREATE_NO_WINDOW};
            cmd.creation_flags(CREATE_NO_WINDOW | BELOW_NORMAL_PRIORITY_CLASS);
        }
        let mut child = cmd.spawn()?;
        let input = child.stdin.take().ok_or_else(|| std::io::Error::other("no input pipe"))?;
        let stdout = child.stdout.take().ok_or_else(|| std::io::Error::other("no output pipe"))?;
        let (tx, replies) = crossbeam_channel::unbounded();
        std::thread::Builder::new().name("hp-light-host-out".to_owned()).spawn(move || {
            let mut reader = std::io::BufReader::with_capacity(1 << 20, stdout);
            while let Ok(Some(frame)) = read_frame::<_, Reply>(&mut reader) {
                if tx.send(frame).is_err() {
                    break;
                }
            }
        })?;
        log::info!("lighting preview: the lighting process {} started", child.id());
        let priority = hp_app::settings::BakePriority::BelowNormal;
        let process = Process { child, input, replies, started: Instant::now(), priority };
        Ok(Link { process: Some(process), local: None })
    }

    pub(crate) fn local() -> Link {
        let queue: Queue = std::sync::Arc::default();
        let sink = queue.clone();
        let out: super::engine::Shared = std::sync::Arc::new(move |reply, blob| {
            if let Ok(mut q) = sink.lock() {
                q.push_back((reply, blob));
            }
        });
        Link { process: None, local: Some(Box::new(Local { engine: Engine::default(), queue, out })) }
    }

    pub(crate) fn is_process(&self) -> bool {
        self.process.is_some()
    }

    pub(crate) fn pid(&self) -> Option<u32> {
        self.process.as_ref().map(|p| p.child.id())
    }

    pub(crate) fn uptime(&self) -> Option<Duration> {
        self.process.as_ref().map(|p| p.started.elapsed())
    }

    pub(crate) fn alive(&mut self) -> bool {
        match &mut self.process {
            Some(p) => matches!(p.child.try_wait(), Ok(None)),
            None => true,
        }
    }

    pub(crate) fn set_priority(&mut self, priority: hp_app::settings::BakePriority) {
        if let Some(p) = self.process.as_mut().filter(|p| p.priority != priority) {
            p.priority = priority;
            super::super::memory::set_child_priority(&p.child, priority);
        }
    }

    pub(crate) fn memory(&self) -> Option<super::super::memory::Usage> {
        self.process.as_ref().and_then(|p| super::super::memory::of_child(&p.child))
    }

    pub(crate) fn send(
        &mut self,
        req: Request,
        blob: &[u8],
        cancelled: &(dyn Fn() -> bool + Sync),
    ) -> Result<(), Gone> {
        if let Some(p) = &mut self.process {
            let mut frame = Vec::with_capacity(blob.len() + 256);
            write_frame(&mut frame, &req, blob).map_err(|_| Gone::Exited)?;
            return p.input.write_all(&frame).and_then(|()| p.input.flush()).map_err(|_| Gone::Exited);
        }
        if let Some(local) = &mut self.local {
            let Local { engine, out, .. } = &mut **local;
            engine.handle(req, blob.to_vec(), out, cancelled);
        }
        Ok(())
    }

    pub(crate) fn recv(&mut self, timeout: Duration) -> Result<Option<(Reply, Vec<u8>)>, Gone> {
        if let Some(p) = &mut self.process {
            return match p.replies.recv_timeout(timeout) {
                Ok(frame) => Ok(Some(frame)),
                Err(RecvTimeoutError::Timeout) => {
                    if matches!(p.child.try_wait(), Ok(Some(_)) | Err(_)) && p.replies.is_empty() {
                        Err(Gone::Exited)
                    } else {
                        Ok(None)
                    }
                }
                Err(RecvTimeoutError::Disconnected) => Err(Gone::Exited),
            };
        }
        match &self.local {
            Some(local) => Ok(local.queue.lock().ok().and_then(|mut q| q.pop_front())),
            None => Err(Gone::Exited),
        }
    }

    pub(crate) fn cancel(&mut self, job: u64) {
        let never = || false;
        let _ = self.send(Request::Cancel { job }, &[], &never);
    }
}

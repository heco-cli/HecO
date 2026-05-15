use std::io::{BufReader, Read};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandOutputEvent {
    Line(String),
    Overwrite(String),
}

pub struct CommandRunner {
    working_dir: PathBuf,
    env: std::collections::HashMap<String, String>,
}

impl CommandRunner {
    pub fn new(working_dir: PathBuf) -> Self {
        Self {
            working_dir,
            env: std::collections::HashMap::new(),
        }
    }

    pub fn env(mut self, key: &str, value: &str) -> Self {
        self.env.insert(key.to_string(), value.to_string());
        self
    }

    pub fn run_with_events<F>(
        &self,
        program: &str,
        args: &[&str],
        mut handler: F,
    ) -> anyhow::Result<()>
    where
        F: FnMut(CommandOutputEvent),
    {
        if crate::output::verbose_level() >= 1 {
            crate::output::line(format!("Running `{} {}`", program, args.join(" ")));
        }

        let (reader, writer) = os_pipe::pipe()?;

        let mut cmd = Command::new(program);
        let child = cmd
            .args(args)
            .current_dir(&self.working_dir)
            .stdin(Stdio::null())
            .stdout(writer.try_clone()?)
            .stderr(writer);

        for (key, value) in &self.env {
            child.env(key, value);
        }

        let mut child = child.spawn()?;

        // 丢弃 Command 对象以关闭父进程中的管道写入端
        drop(cmd);

        // 读取输出并按行处理，同时处理回车符
        let mut reader = BufReader::new(reader);
        let mut line = String::new();

        loop {
            let mut buf = [0; 1024];
            match reader.read(&mut buf) {
                Ok(0) => {
                    flush_buffer(&mut line, false, &mut handler);
                    break;
                }
                Ok(n) => {
                    if let Ok(s) = std::str::from_utf8(&buf[..n]) {
                        dispatch_output_chunk(s, &mut line, &mut handler);
                    }
                }
                Err(_) => break,
            }
        }

        // 等待子进程完成
        let status = child.wait()?;

        if !status.success() {
            anyhow::bail!("command failed: {} {}", program, args.join(" "));
        }

        Ok(())
    }

    pub fn run_captured_merged_with_timeout(
        &self,
        program: &str,
        args: &[&str],
        timeout: Option<std::time::Duration>,
    ) -> anyhow::Result<std::process::Output> {
        let (reader, writer) = os_pipe::pipe()?;

        let mut cmd = Command::new(program);
        let child = cmd
            .args(args)
            .current_dir(&self.working_dir)
            .stdin(Stdio::null())
            .stdout(writer.try_clone()?)
            .stderr(writer);

        for (key, value) in &self.env {
            child.env(key, value);
        }

        let mut child = child.spawn()?;

        // 丢弃 Command 对象以关闭父进程中的管道写入端，
        // 否则即使子进程退出，读操作也会因为写入端未关闭而一直阻塞。
        drop(cmd);

        // 创建一个通道来接收输出
        let (output_tx, output_rx) = mpsc::channel();

        // 启动一个线程来读取输出
        std::thread::spawn(move || {
            let mut reader = BufReader::new(reader);
            let mut buf = [0u8; 4096];
            let mut output = Vec::new();

            // 持续读取直到管道关闭
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => output.extend_from_slice(&buf[..n]),
                    Err(_) => break,
                }
            }

            // 发送输出到通道
            let _ = output_tx.send(output);
        });

        // 根据是否设置了超时来决定是否等待子进程完成
        let status = if timeout.is_some() {
            // 设置了超时，不等待子进程完成
            // 子进程会继续运行，与主进程分离
            use std::process::ExitStatus;
            #[cfg(unix)]
            let status = ExitStatus::default();
            #[cfg(windows)]
            let status = ExitStatus::default();
            status
        } else {
            // 没有设置超时，等待子进程完成
            child.wait()?
        };

        // 尝试读取输出
        let output = if let Some(timeout) = timeout {
            // 设置了超时，尝试读取输出但不阻塞
            output_rx.recv_timeout(timeout).unwrap_or_default()
        } else {
            // 没有设置超时，等待输出读取完成
            output_rx.recv().unwrap_or_default()
        };

        Ok(std::process::Output {
            status,
            stdout: output,
            stderr: Vec::new(),
        })
    }
}

fn dispatch_output_chunk<F>(chunk: &str, line_buffer: &mut String, handler: &mut F)
where
    F: FnMut(CommandOutputEvent),
{
    for c in chunk.chars() {
        match c {
            '\n' => flush_buffer(line_buffer, false, handler),
            '\r' => flush_buffer(line_buffer, true, handler),
            _ => line_buffer.push(c),
        }
    }
}

fn flush_buffer<F>(line_buffer: &mut String, overwrite: bool, handler: &mut F)
where
    F: FnMut(CommandOutputEvent),
{
    if line_buffer.is_empty() {
        return;
    }

    let content = std::mem::take(line_buffer);
    let event = if overwrite {
        CommandOutputEvent::Overwrite(content)
    } else {
        CommandOutputEvent::Line(content)
    };
    handler(event);
}

#[cfg(test)]
mod tests {
    use super::{CommandOutputEvent, dispatch_output_chunk, flush_buffer};

    #[test]
    fn splits_line_and_overwrite_events() {
        let mut buffer = String::new();
        let mut events = Vec::new();

        dispatch_output_chunk("Working...[12%]\rDone line\n", &mut buffer, &mut |event| {
            events.push(event);
        });

        assert_eq!(
            events,
            vec![
                CommandOutputEvent::Overwrite("Working...[12%]".to_string()),
                CommandOutputEvent::Line("Done line".to_string()),
            ]
        );
    }

    #[test]
    fn flushes_remaining_content_as_line() {
        let mut buffer = String::new();
        let mut events = Vec::new();

        dispatch_output_chunk("tail output", &mut buffer, &mut |event| events.push(event));
        flush_buffer(&mut buffer, false, &mut |event| events.push(event));

        assert_eq!(
            events,
            vec![CommandOutputEvent::Line("tail output".to_string())]
        );
    }
}

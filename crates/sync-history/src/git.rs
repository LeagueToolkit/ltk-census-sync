//! git as a process. Every call names the repository with `--git-dir`: `git -C` on a directory
//! that is not a repository falls through to the enclosing one, and the working clone sits inside
//! this checkout.

use std::io::{BufRead, BufReader, BufWriter, Read, Write};
use std::process::{Child, ChildStdin, Command, Stdio};

use camino::{Utf8Path, Utf8PathBuf};

use crate::Error;

/// A repository, by its git directory.
#[derive(Debug, Clone)]
pub struct Git {
    dir: Utf8PathBuf,
}

impl Git {
    /// The repository whose git directory is `dir`.
    pub fn open(dir: &Utf8Path) -> Result<Self, Error> {
        let git = Self { dir: dir.to_path_buf() };
        let bare = git.run(&["rev-parse", "--is-bare-repository"])?;
        if bare.trim() != "true" {
            return Err(Error::Git { args: format!("{dir}"), message: "not a bare repository".into() });
        }
        Ok(git)
    }

    fn command(&self) -> Command {
        let mut command = Command::new("git");
        command.arg(format!("--git-dir={}", self.dir));
        command
    }

    /// A command's output.
    pub fn run(&self, args: &[&str]) -> Result<String, Error> {
        let out = self.command().args(args).output()?;
        if !out.status.success() {
            return Err(Error::Git { args: args.join(" "), message: String::from_utf8_lossy(&out.stderr).trim().to_string() });
        }
        String::from_utf8(out.stdout).map_err(|_| Error::Git { args: args.join(" "), message: "printed bytes that are not UTF-8".into() })
    }

    /// The object id a revision names.
    pub fn rev_parse(&self, rev: &str) -> Result<String, Error> {
        Ok(self.run(&["rev-parse", "--verify", "--end-of-options", rev])?.trim().to_string())
    }

    /// Moves a ref to `new`, only if it is at `old` (`None`: only if it does not exist).
    pub fn update_ref(&self, name: &str, new: &str, old: Option<&str>) -> Result<(), Error> {
        let zero = "0".repeat(40);
        self.run(&["update-ref", name, new, old.unwrap_or(&zero)]).map(drop)
    }

    /// Sets a ref to `new`, wherever it was.
    pub fn reset_ref(&self, name: &str, new: &str) -> Result<(), Error> {
        self.run(&["update-ref", name, new]).map(drop)
    }

    /// Deletes a ref.
    pub fn delete_ref(&self, name: &str) -> Result<(), Error> {
        self.run(&["update-ref", "-d", name]).map(drop)
    }

    /// Every file of a commit's tree, as `ls-tree -r -z` prints them: `<mode> blob <id>\t<path>\0`.
    pub fn ls_tree(&self, commit: &str) -> Result<String, Error> {
        self.run(&["ls-tree", "-r", "-z", "--full-tree", commit])
    }

    /// Objects' contents by id, each handed to `each` in the order asked. The requests are written
    /// from a thread, so they and the replies stream together.
    pub fn cat(&self, ids: &[&str], mut each: impl FnMut(usize, &[u8]) -> Result<(), Error>) -> Result<(), Error> {
        let mut child = self.command().args(["cat-file", "--batch"]).stdin(Stdio::piped()).stdout(Stdio::piped()).spawn()?;
        let stdin = child.stdin.take().expect("a piped stdin");
        let request: Vec<String> = ids.iter().map(|id| format!("{id}\n")).collect();
        let writer = std::thread::spawn(move || -> std::io::Result<()> {
            let mut stdin = BufWriter::new(stdin);
            for line in request {
                stdin.write_all(line.as_bytes())?;
            }
            stdin.flush()
        });
        let mut stdout = BufReader::with_capacity(1 << 20, child.stdout.take().expect("a piped stdout"));
        let mut header = String::new();
        let mut buf = Vec::new();
        let result = (|| -> Result<(), Error> {
            for (i, id) in ids.iter().enumerate() {
                header.clear();
                stdout.read_line(&mut header)?;
                let size: usize = header
                    .split_whitespace()
                    .nth(2)
                    .and_then(|s| s.parse().ok())
                    .ok_or_else(|| Error::Git { args: "cat-file --batch".into(), message: format!("{id}: {}", header.trim()) })?;
                buf.resize(size + 1, 0);
                stdout.read_exact(&mut buf)?;
                each(i, &buf[..size])?;
            }
            Ok(())
        })();
        if result.is_err() {
            let _ = child.kill();
        }
        let written = writer.join().expect("the cat-file writer does not panic");
        child.wait()?;
        result?;
        written?;
        Ok(())
    }

    /// `git fast-import`, reading the stream written to it.
    pub fn fast_import(&self) -> Result<FastImport, Error> {
        // `--force`: the stream writes only a scratch ref, which a run that stopped can leave
        // behind on another commit.
        let mut child = self.command().args(["fast-import", "--quiet", "--done", "--force"]).stdin(Stdio::piped()).spawn()?;
        let stdin = child.stdin.take().expect("a piped stdin");
        // One large pipe write fails on Windows (os error 87), and a build's stream can be
        // gigabytes: it goes through a buffer as it is produced.
        Ok(FastImport { child, stream: Some(BufWriter::with_capacity(8 << 20, stdin)) })
    }
}

/// A running `git fast-import`. Dropped without `finish`, it is killed, and the refs its stream
/// named are not updated.
pub struct FastImport {
    child: Child,
    stream: Option<BufWriter<ChildStdin>>,
}

impl FastImport {
    /// Where the stream goes.
    pub fn stream(&mut self) -> &mut BufWriter<ChildStdin> {
        self.stream.as_mut().expect("the stream is open until finish")
    }

    /// Ends the stream with `done` and waits for the import.
    pub fn finish(mut self) -> Result<(), Error> {
        let mut stream = self.stream.take().expect("the stream is open until finish");
        stream.write_all(b"done\n")?;
        stream.flush()?;
        drop(stream);
        let status = self.child.wait()?;
        if !status.success() {
            return Err(Error::Git { args: "fast-import".into(), message: format!("exited with {status}") });
        }
        Ok(())
    }
}

impl Drop for FastImport {
    fn drop(&mut self) {
        if self.stream.take().is_some() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

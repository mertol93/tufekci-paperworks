use std::io;
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, ExitStatus};

pub(crate) struct ManagedChild {
    child: Child,
    tree: Option<ProcessTree>,
}

impl ManagedChild {
    pub(crate) fn spawn(command: &mut Command) -> io::Result<Self> {
        let prepared = PreparedProcessTree::prepare(command)?;
        let mut child = command.spawn()?;
        let tree = match prepared.attach(&child) {
            Ok(tree) => tree,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(io::Error::new(
                    error.kind(),
                    format!("The external process could not be isolated: {error}"),
                ));
            }
        };
        Ok(Self {
            child,
            tree: Some(tree),
        })
    }

    pub(crate) fn take_stdin(&mut self) -> Option<ChildStdin> {
        self.child.stdin.take()
    }

    pub(crate) fn take_stdout(&mut self) -> Option<ChildStdout> {
        self.child.stdout.take()
    }

    pub(crate) fn take_stderr(&mut self) -> Option<ChildStderr> {
        self.child.stderr.take()
    }

    pub(crate) fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        let status = self.child.try_wait()?;
        if status.is_some() {
            self.finish_tree();
        }
        Ok(status)
    }

    pub(crate) fn wait(&mut self) -> io::Result<ExitStatus> {
        let result = self.child.wait();
        self.finish_tree();
        result
    }

    pub(crate) fn terminate_tree(&mut self) -> io::Result<()> {
        let tree_result = self
            .tree
            .as_ref()
            .map(ProcessTree::terminate)
            .unwrap_or(Ok(()));
        let child_result = self.child.kill();
        match (tree_result, child_result) {
            (Ok(()), _) | (Err(_), Ok(())) => Ok(()),
            (Err(tree_error), Err(_)) => Err(tree_error),
        }
    }

    fn finish_tree(&mut self) {
        if let Some(tree) = self.tree.take() {
            let _ = tree.terminate();
        }
    }
}

impl Drop for ManagedChild {
    fn drop(&mut self) {
        if self.tree.is_some() {
            let _ = self.terminate_tree();
            let _ = self.child.wait();
            self.finish_tree();
        }
    }
}

#[cfg(windows)]
struct PreparedProcessTree {
    job: OwnedJob,
}

#[cfg(windows)]
impl PreparedProcessTree {
    fn prepare(command: &mut Command) -> io::Result<Self> {
        use std::os::windows::process::CommandExt;
        use windows_sys::Win32::System::Threading::CREATE_SUSPENDED;

        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW | CREATE_SUSPENDED);
        Ok(Self {
            job: OwnedJob::new()?,
        })
    }

    fn attach(self, child: &Child) -> io::Result<ProcessTree> {
        self.job.assign(child)?;
        resume_suspended_process(child.id())?;
        Ok(ProcessTree { job: self.job })
    }
}

#[cfg(windows)]
struct ProcessTree {
    job: OwnedJob,
}

#[cfg(windows)]
impl ProcessTree {
    fn terminate(&self) -> io::Result<()> {
        self.job.terminate()
    }
}

#[cfg(windows)]
struct OwnedJob {
    handle: windows_sys::Win32::Foundation::HANDLE,
}

#[cfg(windows)]
impl OwnedJob {
    fn new() -> io::Result<Self> {
        use std::mem::size_of;
        use std::ptr;
        use windows_sys::Win32::System::JobObjects::{
            CreateJobObjectW, JobObjectExtendedLimitInformation, SetInformationJobObject,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        };

        // SAFETY: both optional pointers are null, so Windows creates an unnamed
        // job with default security attributes and returns an owned handle.
        let handle = unsafe { CreateJobObjectW(ptr::null(), ptr::null()) };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        let job = Self { handle };
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        // SAFETY: the handle is valid and the information pointer and byte count
        // describe a live JOBOBJECT_EXTENDED_LIMIT_INFORMATION value.
        let configured = unsafe {
            SetInformationJobObject(
                job.handle,
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        };
        if configured == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(job)
    }

    fn assign(&self, child: &Child) -> io::Result<()> {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::System::JobObjects::AssignProcessToJobObject;

        // SAFETY: both handles are live for the duration of the call. The child
        // handle remains owned by std::process::Child.
        let assigned =
            unsafe { AssignProcessToJobObject(self.handle, child.as_raw_handle().cast()) };
        if assigned == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    fn terminate(&self) -> io::Result<()> {
        use windows_sys::Win32::System::JobObjects::TerminateJobObject;

        // SAFETY: the job handle is valid until this OwnedJob is dropped.
        let terminated = unsafe { TerminateJobObject(self.handle, 1) };
        if terminated == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
}

#[cfg(windows)]
impl Drop for OwnedJob {
    fn drop(&mut self) {
        use windows_sys::Win32::Foundation::CloseHandle;

        // SAFETY: this type uniquely owns the handle and closes it exactly once.
        let _ = unsafe { CloseHandle(self.handle) };
    }
}

#[cfg(windows)]
fn resume_suspended_process(process_id: u32) -> io::Result<()> {
    use std::mem::size_of;
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Thread32First, Thread32Next, TH32CS_SNAPTHREAD, THREADENTRY32,
    };
    use windows_sys::Win32::System::Threading::{OpenThread, ResumeThread, THREAD_SUSPEND_RESUME};

    // SAFETY: the call takes scalar values and returns a new snapshot handle.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    let snapshot = OwnedKernelHandle(snapshot);
    let mut entry = THREADENTRY32 {
        dwSize: size_of::<THREADENTRY32>() as u32,
        ..THREADENTRY32::default()
    };
    // SAFETY: snapshot is valid and entry points to an initialised structure with
    // the required byte size.
    if unsafe { Thread32First(snapshot.0, &mut entry) } == 0 {
        return Err(io::Error::last_os_error());
    }

    let mut resumed = false;
    loop {
        if entry.th32OwnerProcessID == process_id {
            // SAFETY: the thread ID came from the live system snapshot. The
            // returned handle is checked and then uniquely owned.
            let thread = unsafe { OpenThread(THREAD_SUSPEND_RESUME, 0, entry.th32ThreadID) };
            if thread.is_null() {
                return Err(io::Error::last_os_error());
            }
            let thread = OwnedKernelHandle(thread);
            // SAFETY: thread is a valid handle opened with suspend/resume access.
            if unsafe { ResumeThread(thread.0) } == u32::MAX {
                return Err(io::Error::last_os_error());
            }
            resumed = true;
        }

        // SAFETY: snapshot and entry remain valid for the complete enumeration.
        if unsafe { Thread32Next(snapshot.0, &mut entry) } == 0 {
            break;
        }
    }

    if resumed {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            "The suspended child process thread could not be located.",
        ))
    }
}

#[cfg(windows)]
struct OwnedKernelHandle(windows_sys::Win32::Foundation::HANDLE);

#[cfg(windows)]
impl Drop for OwnedKernelHandle {
    fn drop(&mut self) {
        use windows_sys::Win32::Foundation::CloseHandle;

        // SAFETY: this type uniquely owns the handle and closes it exactly once.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

#[cfg(unix)]
struct PreparedProcessTree;

#[cfg(unix)]
impl PreparedProcessTree {
    fn prepare(command: &mut Command) -> io::Result<Self> {
        use std::os::unix::process::CommandExt;

        command.process_group(0);
        Ok(Self)
    }

    fn attach(self, child: &Child) -> io::Result<ProcessTree> {
        let process_group = i32::try_from(child.id()).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "The child process identifier exceeded the platform range.",
            )
        })?;
        Ok(ProcessTree { process_group })
    }
}

#[cfg(unix)]
struct ProcessTree {
    process_group: i32,
}

#[cfg(unix)]
impl ProcessTree {
    fn terminate(&self) -> io::Result<()> {
        // SAFETY: a negative PID targets the process group created for this
        // child. SIGKILL matches std::process::Child::kill on Unix.
        let result = unsafe { libc::kill(-self.process_group, libc::SIGKILL) };
        if result == 0 {
            return Ok(());
        }
        let error = io::Error::last_os_error();
        if error.raw_os_error() == Some(libc::ESRCH) {
            Ok(())
        } else {
            Err(error)
        }
    }
}

#[cfg(not(any(unix, windows)))]
struct PreparedProcessTree;

#[cfg(not(any(unix, windows)))]
impl PreparedProcessTree {
    fn prepare(_command: &mut Command) -> io::Result<Self> {
        Ok(Self)
    }

    fn attach(self, _child: &Child) -> io::Result<ProcessTree> {
        Ok(ProcessTree)
    }
}

#[cfg(not(any(unix, windows)))]
struct ProcessTree;

#[cfg(not(any(unix, windows)))]
impl ProcessTree {
    fn terminate(&self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::Stdio;
    use std::thread;
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    #[test]
    fn terminating_a_managed_child_stops_its_descendant() {
        let directory = TestDirectory::new();
        let heartbeat = directory.path.join("heartbeat");
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .arg("--exact")
            .arg("child_process::tests::process_tree_parent_helper")
            .arg("--nocapture")
            .env("PAPERWORKS_PROCESS_TREE_PARENT", "1")
            .env("PAPERWORKS_PROCESS_TREE_HEARTBEAT", &heartbeat)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut child = ManagedChild::spawn(&mut command).unwrap();

        let first = wait_for_heartbeat_change(&heartbeat, None);
        let second = wait_for_heartbeat_change(&heartbeat, Some(first));
        assert!(second > first);

        child.terminate_tree().unwrap();
        child.wait().unwrap();
        thread::sleep(Duration::from_millis(250));
        let stopped_at = read_heartbeat(&heartbeat).unwrap();
        thread::sleep(Duration::from_millis(300));
        assert_eq!(read_heartbeat(&heartbeat), Some(stopped_at));
    }

    #[test]
    fn process_tree_parent_helper() {
        if std::env::var_os("PAPERWORKS_PROCESS_TREE_PARENT").is_none() {
            return;
        }
        let heartbeat =
            PathBuf::from(std::env::var_os("PAPERWORKS_PROCESS_TREE_HEARTBEAT").unwrap());
        let mut grandchild = Command::new(std::env::current_exe().unwrap());
        grandchild
            .arg("--exact")
            .arg("child_process::tests::process_tree_grandchild_helper")
            .arg("--nocapture")
            .env("PAPERWORKS_PROCESS_TREE_GRANDCHILD", "1")
            .env("PAPERWORKS_PROCESS_TREE_HEARTBEAT", heartbeat)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut grandchild = grandchild.spawn().unwrap();
        let _reaper = thread::spawn(move || {
            let _ = grandchild.wait();
        });
        loop {
            thread::sleep(Duration::from_secs(30));
        }
    }

    #[test]
    fn process_tree_grandchild_helper() {
        if std::env::var_os("PAPERWORKS_PROCESS_TREE_GRANDCHILD").is_none() {
            return;
        }
        let heartbeat =
            PathBuf::from(std::env::var_os("PAPERWORKS_PROCESS_TREE_HEARTBEAT").unwrap());
        for value in 1_u64.. {
            fs::write(&heartbeat, value.to_string()).unwrap();
            thread::sleep(Duration::from_millis(35));
        }
    }

    fn wait_for_heartbeat_change(path: &Path, previous: Option<u64>) -> u64 {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if let Some(value) = read_heartbeat(path) {
                if previous.is_none_or(|previous| value > previous) {
                    return value;
                }
            }
            thread::sleep(Duration::from_millis(20));
        }
        panic!("The descendant process did not update its heartbeat.");
    }

    fn read_heartbeat(path: &Path) -> Option<u64> {
        fs::read_to_string(path).ok()?.trim().parse().ok()
    }

    struct TestDirectory {
        path: PathBuf,
    }

    impl TestDirectory {
        fn new() -> Self {
            let stamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "tufekci-paperworks-process-tree-{}-{stamp}",
                std::process::id()
            ));
            fs::create_dir(&path).unwrap();
            Self { path }
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

//! Console-less child processes: the engine, health tools and DPAPI helpers all
//! spawn CLIs whose windows must never flash.

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Spawn a console executable with no visible console window. `creation_flags`
/// REPLACES the whole flag word (std does `self.flags = flags`), so a future
/// site that needs other flags must OR them in:
/// `cmd.creation_flags(cmd.get_creation_flags() | CREATE_NO_WINDOW)`.
pub fn spawn_hidden(cmd: &mut std::process::Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    let _ = cmd;
}

/// Spawn a child whose lifetime is the app's: on Windows it joins a
/// process-wide job object that kills it when the app dies, and on Unix it
/// gets its own process group (Linux also asks the kernel for a SIGKILL when
/// the parent dies). A child that cannot be tied is killed and the spawn
/// fails, so an engine never runs outside the tie.
pub fn spawn_tied(cmd: &mut std::process::Command) -> std::io::Result<std::process::Child> {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt as _;
        cmd.process_group(0);
        // Linux ties the child's life to the parent's; macOS has no
        // PR_SET_PDEATHSIG equivalent, so there the group is the only tie
        // and the normal Drop stop is still the way a child ends.
        #[cfg(target_os = "linux")]
        {
            // SAFETY: pre_exec runs between fork and exec, so the closure may
            // only call async-signal-safe functions; prctl is one and touches
            // no memory shared with other threads.
            unsafe {
                cmd.pre_exec(|| {
                    if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) == 0 {
                        Ok(())
                    } else {
                        Err(std::io::Error::last_os_error())
                    }
                });
            }
            // The signal fires when the parent *thread* exits, so engine
            // spawns must stay on a thread that lives as long as the app.
        }
    }
    #[cfg(windows)]
    let mut child = cmd.spawn()?;
    #[cfg(not(windows))]
    let child = cmd.spawn()?;
    #[cfg(windows)]
    if let Err(error) = tie_to_app(&child) {
        let _ = child.kill();
        let _ = child.wait();
        return Err(error);
    }
    Ok(child)
}

#[cfg(windows)]
static APP_JOB: std::sync::Mutex<Option<usize>> = std::sync::Mutex::new(None);

#[cfg(windows)]
fn app_job() -> std::io::Result<windows::Win32::Foundation::HANDLE> {
    use windows::Win32::Foundation::HANDLE;

    let mut slot = APP_JOB.lock().unwrap_or_else(|poison| poison.into_inner());
    if let Some(raw) = *slot {
        return Ok(HANDLE(raw as *mut _));
    }
    let job = create_job()?;
    *slot = Some(job.0 as usize);
    Ok(job)
}

#[cfg(windows)]
fn create_job() -> std::io::Result<windows::Win32::Foundation::HANDLE> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::JobObjects::{
        CreateJobObjectW, JobObjectExtendedLimitInformation, SetInformationJobObject,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };

    // SAFETY: both calls take plain handles and pointers to stack data, return
    // errors instead of panicking, and the job handle is kept for the process.
    unsafe {
        let job = CreateJobObjectW(None, windows::core::PCWSTR::null()).map_err(|error| {
            std::io::Error::other(format!("could not create the engine job ({error})"))
        })?;
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let armed = SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &limits as *const _ as *const core::ffi::c_void,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        );
        if let Err(error) = armed {
            let _ = CloseHandle(job);
            return Err(std::io::Error::other(format!(
                "could not arm the engine job ({error})"
            )));
        }
        Ok(job)
    }
}

#[cfg(windows)]
fn tie_to_app(child: &std::process::Child) -> std::io::Result<()> {
    use std::os::windows::io::AsRawHandle as _;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::JobObjects::AssignProcessToJobObject;

    let job = app_job()?;
    // SAFETY: the job lives for the process and the child handle is borrowed
    // for the duration of the call; the child is still running.
    unsafe { AssignProcessToJobObject(job, HANDLE(child.as_raw_handle())) }.map_err(|error| {
        std::io::Error::other(format!(
            "engine started but could not join the app job ({error})"
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn spawned_child_is_in_the_app_job() {
        use std::os::windows::io::AsRawHandle as _;
        use windows::Win32::Foundation::{BOOL, HANDLE};
        use windows::Win32::System::JobObjects::IsProcessInJob;

        let comspec = std::env::var("ComSpec").unwrap_or_else(|_| "cmd.exe".to_string());
        let mut cmd = std::process::Command::new(comspec);
        cmd.args(["/c", "ping -n 30 127.0.0.1 >NUL"])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        spawn_hidden(&mut cmd);
        let mut child = spawn_tied(&mut cmd).expect("child spawns and joins the app job");
        let job = app_job().expect("job object exists");
        let mut in_job = BOOL(0);
        // SAFETY: the job lives for the process and the child is still
        // running, so both handles stay valid for the call.
        unsafe { IsProcessInJob(HANDLE(child.as_raw_handle()), job, &mut in_job) }
            .expect("IsProcessInJob succeeds");
        assert_ne!(in_job.0, 0, "child is in the app job");
        let _ = child.kill();
        let _ = child.wait();
    }

    #[cfg(unix)]
    #[test]
    fn spawned_child_leads_its_own_process_group() {
        let mut cmd = std::process::Command::new("sleep");
        cmd.arg("30")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        let mut child = spawn_tied(&mut cmd).expect("child spawns");
        let pid = child.id() as i32;
        // SAFETY: getpgid reads only the pid; the child is still running.
        let pgid = unsafe { libc::getpgid(pid) };
        assert_eq!(pgid, pid, "child leads its own process group");
        let _ = child.kill();
        let _ = child.wait();
    }
}

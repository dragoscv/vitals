//! Prints what a process started by the IFEO hook actually sees.
//!
//! Point the `taskmgr.exe` debugger at this binary, press Ctrl+Shift+Esc or
//! run `vitals-desktop --launch-real-taskmgr`, and the file it appends to
//! tells you the token, the arguments and whether the hand-off would fire.
//! It is the only way to observe the process Windows starts for you: it has
//! no console, and its parent may already be gone. This is how the parent-
//! PID approach was ruled out: the elevated hop arrives with no parent at
//! all, because `AppInfo` spawns it, not the first Task Manager.

fn main() {
    #[cfg(windows)]
    {
        let args: Vec<String> = std::env::args().collect();
        let hop = vitals_win::actions::is_task_manager_elevation_hop();
        let elevated = vitals_win::actions::is_elevated();
        let status = vitals_win::actions::replacement_status();
        let line = format!(
            "{:?}\nis_task_manager_elevation_hop = {hop}\nelevated = {elevated}\nstatus = {status:?}\nargs = {args:?}\n\n",
            std::time::SystemTime::now()
        );
        let path = std::env::temp_dir().join("vitals-prove-taskmgr-parent.txt");
        let _ = std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(&path)
            .and_then(|mut f| std::io::Write::write_all(&mut f, line.as_bytes()));
        print!("{line}");
        println!("(appended to {})", path.display());
    }
}

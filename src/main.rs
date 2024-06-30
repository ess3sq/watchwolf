use std::{collections::HashMap, fs::metadata, io::ErrorKind, path::Path, process::Command, thread::sleep, time::{Duration, SystemTime}};

const FILE_FORMATTED_LIST_PLACEHOLDER: &'static str = "%F";
const FILE_SEQUENCE_PLACEHOLDER: &'static str = "%f";

const SAMPLING_PERIOD_MILLIS: u64 = 50;

fn print_help() {
    eprintln!("{} - watch for file changes
options:
    --help,    -h   print and exit
    --files,   -f   begin file list
    --command, -c   begin command
    --silent,  -s   silent mode (verbose is on by default)
format:
    the command string supports the following placeholders:
        %f    expands to a space-separated list of file names;
        %F    expands to a `, `-separated list of file names;
    the file names in the list correspond to those of files or directories which have changed since the last update.",
              std::env::args().next().unwrap());
}

fn format_files_list(changed_files: &[&Path]) -> String {
    if changed_files.len() == 0 {
        panic!("this ain't supposed to happen");
    } 

    let mut list = changed_files[0].to_str().unwrap_or("not-utf-8-path").to_owned();

    for f in &changed_files[1..] {
        list.push_str(&format!(", {}", f.to_str().unwrap_or("not-utf-8-path").to_owned()));
    }

    list
}

fn build_cmd(changed_files: &[&Path], command: &Vec<String>) -> Command {
    let file_list = format_files_list(changed_files);
    let file_sequence = changed_files.iter().map(|p| p.to_str().unwrap_or("not-utf-8-path").to_string()).collect::<Vec<String>>().join(" ");

    if command.len() > 0 {
        let mut cmd = Command::new(&command[0]
                                .replace(FILE_FORMATTED_LIST_PLACEHOLDER, &file_list)
                                .replace(FILE_SEQUENCE_PLACEHOLDER, &file_sequence));
        for x in &command[1..] {
            cmd.arg(x
                    .replace(FILE_FORMATTED_LIST_PLACEHOLDER, &file_list)
                    .replace(FILE_SEQUENCE_PLACEHOLDER, &file_sequence));
        }
        return cmd;
    }

    let mut cmd = Command::new("echo");
    cmd.args(&["{file_list}", "changed"]);

    cmd
}

fn process_changed_files<'a>(all_files: &mut HashMap<&'a Path, FileState>) -> Option<Vec<&'a Path>> {
    let mut changes = vec![]; 

    for (f, fs) in all_files.iter_mut() {
        let curr_fs = FileState::of(f);
        if fs.has_changed(&curr_fs) {
            changes.push(*f);
            *fs = curr_fs;
        }
    }

    if changes.len() > 0 {
        Some(changes)
    } else {
        None
    }
}

enum FileState {
    IsFile(SystemTime),
    IsDir(SystemTime),
    IsOther(SystemTime),
    Inexistent(SystemTime),
    NoPerm(SystemTime),
}

impl FileState {
    fn of(path: &Path) -> FileState {
        let md = match metadata(path) {
            Err(e) => match e.kind() {
                ErrorKind::NotFound => return FileState::Inexistent(SystemTime::UNIX_EPOCH),
                ErrorKind::PermissionDenied => return FileState::NoPerm(SystemTime::UNIX_EPOCH),
                _ => unreachable!("api docs promise this does not happen"),
            },
            Ok(md) => md,
        };

        let tm = md.modified().expect("mod time unavailable on this platform");
        if md.is_file() {
            return FileState::IsFile(tm);
        } else if md.is_dir() {
            return FileState::IsDir(tm);
        }
        return FileState::IsOther(tm);
    }

    fn has_changed(&self, new_state: &FileState) -> bool {
        !self.has_similar_state(new_state) || self.system_time() < new_state.system_time()
    }

    fn has_similar_state(&self, other: &FileState) -> bool {
        match (self, other) {
            (FileState::IsFile(_), FileState::IsFile(_)) => true,
            (FileState::IsDir(_), FileState::IsDir(_)) => true,
            (FileState::IsOther(_), FileState::IsOther(_)) => true,
            (FileState::Inexistent(_), FileState::Inexistent(_)) => true,
            (FileState::NoPerm(_), FileState::NoPerm(_)) => true,
            _ => false,
        }
    }

    fn system_time(&self) -> SystemTime {
        match self {
            Self::IsFile(t) | Self::IsDir(t) | Self::IsOther(t) | Self::Inexistent(t) | Self::NoPerm(t) => *t,
        }
    }
}

fn watch(files: Vec<&Path>, command: Vec<String>, silent: bool) {
    let shellcmd = command.join(" ");

    let mut file_state_cache = HashMap::new();
    for f in files {
        file_state_cache.insert(f, FileState::of(f));
    }

    loop {
        sleep(Duration::from_millis(SAMPLING_PERIOD_MILLIS));
        match process_changed_files(&mut file_state_cache) {
            None => continue,
            Some(changes) => {
                let mut cmd = build_cmd(&changes, &command);
                if !silent {
                    eprintln!("# found changes in: {} -- shell: {}", format_files_list(&changes), shellcmd);
                }
                match cmd.status() {
                    Err(e) => eprintln!("# failed to execute command: {e}"),
                    Ok(s) => {
                        eprintln!("# exit status: {}", s.code().map(|x| x.to_string()).unwrap_or("terminated".to_owned()));
                    },
                }
            }
        }
    }
}

fn main() {
    let mut accepting_files = false;
    let mut accepting_command = false;

    let mut files = vec![];
    let mut cmd_args = vec![];

    let mut silent = false;

    for arg in std::env::args().skip(1) {
        if arg.starts_with('-') {
            match arg.as_str() {
                "--help" | "-h" => {
                    print_help();
                    std::process::exit(1);
                },
                "--files" | "-f" => {
                    accepting_files = true;
                    accepting_command = false;
                },
                "--command" | "-c" => {
                    accepting_files = false;
                    accepting_command = true;
                },
                "--silent" | "-s" => {
                    silent = true;
                },
                _ => {
                    eprintln!("invalid option: {arg} -- try --help");
                    std::process::exit(1);
                },
            }
            continue;
        }

        if accepting_files {
            files.push(arg);
        } else if accepting_command {
            cmd_args.push(arg);
        } else {
            eprintln!("unexpected argument: {arg} -- try --help");
            std::process::exit(2);
        }
    }
    if files.len() == 0 {
        eprintln!("no files to watch, aborting");
        std::process::exit(4);
    }

    let files = files.iter().map(Path::new).collect();
    watch(files, cmd_args, silent);
}

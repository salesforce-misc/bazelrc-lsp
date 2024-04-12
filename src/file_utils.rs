use std::path::{Path, PathBuf};

// See https://github.com/bazelbuild/bazel/blob/20c49b49d6d616aeb97d30454656ebbf9cbacd21/src/main/cpp/workspace_layout.cc#L35
const ROOT_FILE_NAME: [&str; 4] = ["MODULE.bazel", "REPO.bazel", "WORKSPACE.bazel", "WORKSPACE"];

pub fn get_workspace_path(path: &Path) -> Option<PathBuf> {
    let mut path_buf = PathBuf::from(path);
    loop {
        if path_buf.is_dir() {
            for file_name in ROOT_FILE_NAME {
                if path_buf.join(file_name).is_file() {
                    return Some(path_buf);
                }
            }
        }
        if !path_buf.pop() {
            break;
        }
    }
    None
}

pub fn resolve_bazelrc_path(file_path: &Path, raw_path: &str) -> Option<PathBuf> {
    let mut path = raw_path.to_string();
    if path.contains("%workspace%") {
        path = path.replace("%workspace%", get_workspace_path(file_path)?.to_str()?);
    }
    Some(file_path.join(Path::new(&path)))
}

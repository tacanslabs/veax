use gen_source_files_list::gen_source_files_list;
use ver_from_git::version_from_git;

fn main() {
    gen_source_files_list().unwrap();
    version_from_git();
}

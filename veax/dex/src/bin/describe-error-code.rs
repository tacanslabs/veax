use veax_dex::describe_error_code;

fn main() {
    let code: i32 = std::env::args()
        .nth(1)
        .expect("Please specify error code to describe")
        .parse()
        .expect("Could not parse argument as error code");
    println!("{}", describe_error_code(code));
}

use serde::Serialize;

pub fn print<T: Serialize + std::fmt::Debug>(value: &T, json: bool, human: impl FnOnce(&T)) {
    if json {
        println!("{}", serde_json::to_string_pretty(value).unwrap());
    } else {
        human(value);
    }
}

pub fn print_list<T: Serialize + std::fmt::Debug>(
    values: &[T],
    json: bool,
    human: impl Fn(&T),
) {
    if json {
        println!("{}", serde_json::to_string_pretty(values).unwrap());
    } else {
        for v in values {
            human(v);
        }
    }
}

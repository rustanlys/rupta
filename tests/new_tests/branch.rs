// A test that uses branching for a function to show flow sensitivity

fn main() {
    let number = 10;
    let result;

    let option1 = 1;
    let option2 = 2;

    if number > 5 {
        result = &option1;
    } else {
        result = &option2;
    }
}



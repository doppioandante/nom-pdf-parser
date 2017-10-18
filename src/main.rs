extern crate pdf;
#[macro_use]
extern crate nom;

use nom::IResult;
use pdf::parser::{eat_until_next_token, PdfObject, indirect_object};
use std::str::from_utf8;
use std::fs::File;
use std::io::prelude::*;
use std::vec::Vec;
use std::time::{Duration, SystemTime};

fn main() {
    let mut f = File::open("PDF32000_2008.pdf").unwrap();
    let mut content = Vec::new();

    f.read_to_end(&mut content);

    let mut xref = pdf::XRef::new();

    let mut count = 0;
    let mut input = &content[..];
    let now = SystemTime::now();
    loop {
        let res = do_parse!(input,
            eat_until_next_token >>
            apply!(indirect_object, &xref, &content[..]) >>
            ()
        );

        if let IResult::Done(next, _) = res {
            count += 1;
            input = next;
        } else {
            break;
        }
    }
    if let Ok(elapsed) = now.elapsed() {
        println!("Time: {}s", elapsed.as_secs() as f64
                           + elapsed.subsec_nanos() as f64 * 1e-9);
    }

    println!("Objects read: {}", count);
}

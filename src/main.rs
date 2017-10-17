extern crate pdf;
extern crate nom;

use nom::IResult;
use pdf::parser::{PdfObject, object};
use std::str::from_utf8;

fn main() {
    let mut xref = pdf::XRef::new();
    xref.add_entry(8, 213, 0, true);
    let data = include_bytes!("parse_data");
    let res = object(data, &xref, data);
    if let IResult::Done(_, PdfObject::String(vec)) = res {
        println!("{}", from_utf8(vec.as_slice()).unwrap());
    } else if res.is_done() {
        println!("{:?}", res);
    } else {
        println!("Error: {:?}", res);
    }
}

#[macro_use]
extern crate nom;

use nom::{digit};
use std::str::{FromStr, from_utf8};

#[derive(Debug)]
enum PdfObject {
    Boolean(bool),
    Integer(i32), // see limits?
    Real(f64)
}

fn from_bool_literal(s:&[u8]) -> bool {
    if s == b"true" {
        return true;
    }
    if s == b"false" {
        return false;
    }
    unreachable!();
}

named!(boolean <PdfObject>,
    map!(
        map!(
            alt!(tag!("true") | tag!("false")),
            from_bool_literal
        ),
        PdfObject::Boolean
    )
);

named!(integer <PdfObject>,
    map!(
        do_parse!(
            sign: opt!(alt!(tag!("-") | tag!("+"))) >>
            number: map!(
                digit,
                |parsed_digits| {
                    // FIXME: is from_utf8 slow?
                    let mut value = i32::from_str(from_utf8(parsed_digits).unwrap()).unwrap();
                    if let Some(c) = sign {
                        if c == b"-" {
                            value = -value;
                        }
                    }
                    value
                }
            ) >>
            (number)
        ),
        PdfObject::Integer
    )
);

fn main() {
    let data = include_bytes!("parse_data");
    let res = integer(data);
    println!("{:?}", res);
}

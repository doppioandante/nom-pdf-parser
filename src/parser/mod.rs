extern crate nom;

use nom::{digit, hex_digit, IResult, ErrorKind, Needed};
use std::str::{FromStr, from_utf8, from_utf8_unchecked};
use std::collections::HashMap;
use std::fmt::Debug;

use super::XRef;

//mod xref;
//pub use self::xref::*;

// TODO: use enum for error codes (CustomError)

#[derive(Debug, PartialEq)]
pub enum PdfObject {
    Null,
    Boolean(bool),
    Integer(i32),
    Real(f32),
    String(Vec<u8>),
    NameObject(Vec<u8>), //FIXME: max length 127
    Array(Vec<PdfObject>),
    Dictionary(HashMap<Vec<u8>, PdfObject>),
    Stream(Box<PdfObject>, Vec<u8>),
    Indirect(i32, i32, Box<PdfObject>),
    Reference(i32, i32)
}

impl PdfObject {
    fn evaluate_reference(&self, xref: &XRef, data: &[u8]) -> Option<PdfObject> {
        if let &PdfObject::Reference(n, _) = self {
            // FIXME: offset is limited by the integer length (i32)
            // but the xref table has 10 digits, so it should be a i64
            let offset = xref.get_offset(n as u32) as usize;
            let res = indirect_object(&data[offset..], xref, data);
            if let IResult::Done(_, PdfObject::Indirect(_, _, o)) = res {
                return Some(*o)
            }
        }

        // FIXME: should actually be Null
        None
    }
}

pub fn eat_until_next_token(input: &[u8]) -> IResult<&[u8], ()> {
    let mut i = 0;
    while i < input.len() {
        if input[i] == b'%' {
            while i < input.len() && input[i] != b'\r' && input[i] != b'\n' {
                i += 1;
            }
        }
        if !is_space(input[i]) {
            break;
        }

        i += 1;
    }

    IResult::Done(&input[i..], ())
}

macro_rules! fs(
  ($i:expr, $($args:tt)*) => (
    {
      do_parse!($i,
          res: $($args)* >>
          eat_until_next_token >>
          (res)
      )
    }
  )
);

    
pub fn indirect_object<'a>(input: &'a [u8], xref: &XRef, data: &'a [u8]) -> IResult<&'a [u8], PdfObject> {
    map!(input,
        do_parse!(
            number: fs!(digit) >>
            generation: fs!(digit) >>
            fs!(tag!("obj")) >>
            o: fs!(alt!(
                null | boolean | real | integer | apply!(stream_or_dictionary, xref, data) | hex_literal | string_literal | name_object | apply!(array, xref, data)
            )) >>
            tag!("endobj") >>
            (number, generation, o)
        ),
        |(n, g, o)| {
            let number = i32::from_str(unsafe {
                from_utf8_unchecked(n)
            }).unwrap();
            let generation = i32::from_str(unsafe {
                from_utf8_unchecked(g)
            }).unwrap();
            PdfObject::Indirect(number, generation, Box::new(o))
        }
    )
}

//pub fn direct_object<'a>(input: &'a [u8], xref: &XRef, data: &'a [u8]) -> IResult<&'a [u8], PdfObject> {
//    alt!(input,
//        null | boolean | reference | real | integer | apply!(dictionary, xref, data) | hex_literal | string_literal | name_object | apply!(array, xref, data)
//    )
//}

pub fn direct_object<'a>(input: &'a [u8], xref: &XRef, data: &'a [u8]) -> IResult<&'a [u8], PdfObject> {
    if input.len() > 0 {
        match input[0] {
            b'n' => null(input),
            b'0' ... b'9' => alt!(input, reference | real |integer),
            b'-' | b'+' => alt!(input, real | integer),
            b'.' => real(input),
            b't' | b'f' => boolean(input),
            b'(' => string_literal(input),
            b'<' => alt!(input, hex_literal | apply!(dictionary, xref, data)),
            b'/' => name_object(input),
            b'[' => array(input, xref, data),
            b']' => IResult::Error(ErrorKind::Char),
            b'>' => IResult::Error(ErrorKind::Char),
            _ => panic!("Invalid character")
        }
    } else {
        IResult::Incomplete(Needed::Size(1))
    }
}

named!(null <PdfObject>,
    do_parse!(
        tag!("null") >>
        (PdfObject::Null)
    )
);

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

// TODO: use tag instead of char? char with u8 only
named!(integer <PdfObject>,
    map!(
        do_parse!(
            sign: opt!(alt!(char!('-') | char!('+'))) >>
            number: map!(
                digit,
                |parsed_digits| {
                    // FIXME: is from_utf8 slow?
                    let mut value = i32::from_str(unsafe {
                        from_utf8_unchecked(parsed_digits)
                    }).unwrap();
                    if let Some(c) = sign {
                        if c == '-' {
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

named!(real <PdfObject>,
    map!(
        do_parse!(
            sign: opt!(alt!(char!('-') | char!('+'))) >>
            integral: opt!(digit) >>
            char!('.') >>
            result: map!(
                opt!(digit),
                |parsed_digits| {
                    let mut real_parsed = String::new();
                    // FIXME: is from_utf8 slow?
                    if let Some(c) = sign {
                        real_parsed.push(c);
                    }
                    if let Some(parsed) = integral {
                        real_parsed += unsafe{ from_utf8_unchecked(parsed) };
                    }
                    real_parsed += ".";
                    if parsed_digits.is_some() {
                        real_parsed += unsafe{ from_utf8_unchecked(parsed_digits.unwrap()) }
                    }
                    f32::from_str(real_parsed.as_str()).unwrap()
                }
            ) >>
            (result)
        ),
        PdfObject::Real
    )
);

fn from_hex_char(s: u8) -> u8  {
    match s {
        b'0' ... b'9' => s - b'0',
        b'a' ... b'f' => s - b'a' + 10,
        b'A' ... b'F' => s - b'A' + 10,
        _ => unreachable!()
    }
}

fn hex_literal<'a>(s: &'a [u8]) -> IResult<&'a [u8], PdfObject> {
    if s.len() > 0 {
        if s[0] == b'<' {
            let mut result = Vec::new();
            let mut i = 1;
            let mut c = 0;
            while i < s.len() && s[i] != b'>' {
                if !is_hex_digit(s[i]) {
                    return IResult::Error(ErrorKind::Char);
                }
                if i%2 == 0 {
                    result.push(16u8 * c + from_hex_char(s[i]));
                } else {
                    c = from_hex_char(s[i]);
                }
                i += 1;
            }
            if i % 2 != 0 {
                result.push(16u8 * c);
            }

            IResult::Done(&s[i+1..], PdfObject::String(result))
        } else {
            IResult::Error(ErrorKind::Char)
        }
    } else {
        IResult::Incomplete(Needed::Size(1))
    }
}

fn string_literal(ss: &[u8]) -> IResult<&[u8], PdfObject> {
    let opening = char!(ss, '(');
    let s: &[u8];
    match opening {
        IResult::Done(stream, _) => {
            s = stream;
        },
        IResult::Error(e) => {
            return IResult::Error(e) ;
        },
        IResult::Incomplete(needed) => {
            return IResult::Incomplete(needed);
        }
    }

    let mut result = Vec::new();
    let mut i = 0;
    let mut num_par = 1u32;
    let mut escape = false;
    let mut octal = 0u8;
    enum OctalEscape {
        Parsing(u8),
        Complete,
        None
    }
    let mut escape_octal = OctalEscape::None;
    let mut escaped = 0u8;
    while i < s.len() {
        if escape {
            escaped = match s[i] {
                b'n' => b'\n',
                b'r' => b'\r',
                b't' => b'\t',
                b'b' => 0x8,
                b'f' => 0xc,
                b'(' => b'(',
                b')' => b')',
                b'\\' => b'\\',
                b'0' ... b'7' => { // octal
                    escape_octal = OctalEscape::Parsing(3);
                    0u8
                },
                b'\n' => { // trailing \, continue
                    escape = false;
                    i += 1;
                    continue;
                },
                _ => {
                    return IResult::Error(ErrorKind::Custom(2))
                }
            };

            escape = false;
        }

        if let OctalEscape::Parsing(n) = escape_octal {
            if s[i] >= b'0' && s[i] <= b'7' {
                let checked_res = 8u8.checked_mul(octal).and_then(
                    |x| x.checked_add(s[i] - b'0')
                );
                if let Some(val) = checked_res {
                    octal = val;
                } else {
                    // FIXME: how to bail out?
                    return IResult::Error(ErrorKind::Custom(10));
                }
                escape_octal = OctalEscape::Parsing(n-1)
            } else {
                escape_octal = OctalEscape::Complete;
            }
        }

        if let OctalEscape::Complete = escape_octal {
            result.push(octal);
            octal = 0u8;
            escape_octal = OctalEscape::None;
        }

        if escaped != 0 {
            result.push(escaped);
            escaped = 0u8;
        } else if let OctalEscape::None = escape_octal {
            if s[i] == b'\\' {
                escape = true;
            }
            else if s[i] == b'(' {
                num_par += 1;
            }
            else if s[i] == b')' {
                num_par -= 1;
                if num_par == 0 {
                    i += 1;
                    break;
                }
            }
            if !escape {
                result.push(s[i]);
            }
        }

        i += 1;
    }

    if num_par != 0 {
        return IResult::Incomplete(Needed::Size(1));
    }

    IResult::Done(&s[i..], PdfObject::String(result))
}

fn is_space(c: u8) -> bool {
    match c {
        0x00 | 0x09 | 0x0A | 0x0C | 0x0D | 0x20 => true,
        _ => false
    }
}

fn is_delimiter(c: u8) -> bool {
    match c {
        b'(' |  b')' |  b'<' |  b'>' |  b'[' |  b']' |  b'{' |  b'}' |  b'/' |  b'%' => true,
        _ => false
    }
}

fn is_regular(c: u8) -> bool {
    !is_space(c) && !is_delimiter(c)
}

fn is_hex_digit(x: u8) -> bool {
     (x >= b'0' && x <= b'9')
     || (x >= b'a' && x <= b'f')
     || (x >= b'A' && x <= b'F')
}

fn name_object<'a>(s: &'a [u8]) -> IResult<&'a [u8], PdfObject> {
    if s.len() > 0 {
        if s[0] == b'/' {
            let mut result = Vec::new();
            let mut i = 1;
            let mut hex = 0u8;
            let mut hex_count = 0u8;
            while is_regular(s[i]) {
                if hex_count > 0 {
                    if is_hex_digit(s[i]) {
                        hex = hex*16u8 + from_hex_char(s[i]);
                        hex_count -= 1;

                        if hex_count == 0 {
                            result.push(hex);
                            hex = 0u8;
                        }
                    } else {
                        return IResult::Error(ErrorKind::Char);
                    }
                }
                else if s[i] == b'#' {
                    hex_count = 2;
                }
                else {
                    result.push(s[i]);
                }
                i += 1;
            }

            IResult::Done(&s[i..], PdfObject::NameObject(result))
        } else {
            IResult::Error(ErrorKind::Char)
        }
    } else {
        IResult::Incomplete(Needed::Size(1))
    }
}

pub fn array<'a>(input: &'a [u8], xref: &XRef, data: &'a [u8]) -> IResult<&'a [u8], PdfObject> {
    map!(input,
        delimited!(
            fs!(char!('[')),
            many0!(
                fs!(apply!(direct_object, xref, data))
            ),
            char!(']')
        ),
        PdfObject::Array
    )
}

fn debug_res<O>(r: &IResult<&[u8], O>)
        where O: Debug {
    if let IResult::Done(to_consume, ref o) = *r {
        println!("to consume: {}\nparsed: {:?}", from_utf8(to_consume).unwrap(), o);
    } else {
        println!("error: {:?}", r);
    }
}

pub fn dictionary<'a>(input: &'a [u8], xref: &XRef, data: &'a [u8]) -> IResult<&'a [u8], PdfObject> {
    let mut dict = HashMap::new();
    map!(input,
         delimited!(
             fs!(tag!("<<")),
             many0!(
                 do_parse!(
                     key: fs!(name_object) >>
                         entry: fs!(apply!(direct_object, xref, data)) >>
                         ({
                             match key {
                                 PdfObject::NameObject(key) => match entry {
                                     PdfObject::Null => {},
                                     e => { dict.insert(key, e); }
                                 },
                                 _ => {}
                             }
                             ()
                         })
                 )
             ),
             tag!(">>")
         ),
         |_| {
             PdfObject::Dictionary(dict)
         }
    )
}

//pub fn dictionary<'a>(input: &'a [u8], xref: &XRef, data: &'a [u8]) -> IResult<&'a [u8], PdfObject> {
//   map!(input,
//       delimited!(
//           fs!(tag!("<<")),
//           many0!(
//               do_parse!(
//                   key: fs!(name_object) >>
//                   entry: fs!(apply!(direct_object, xref, data)) >>
//                   (key, entry)
//               )
//           ),
//           tag!(">>")
//       ),
//       |vec| {
//           let mut dict = HashMap::new();
//
//           for (key, entry) in vec {
//               if let PdfObject::NameObject(key) = key {
//                   if entry != PdfObject::Null {
//                       dict.insert(key.into_boxed_slice(), entry);
//                   }
//               }
//           }
//           PdfObject::Dictionary(dict)
//       }
//   )
//}

fn stream_bytes<'a>(input: &'a [u8], dict: &PdfObject, xref: &XRef, data: &'a [u8]) -> IResult<&'a [u8], &'a [u8]> {
    if let PdfObject::Dictionary(ref hash_map) = *dict {
        if let Some(ref object) = hash_map.get(b"Length".as_ref()) {
            // FIXME: maximum length allowed for a stream is 32767
            if let Some(PdfObject::Integer(reflen)) = object.evaluate_reference(xref, data) {
                take!(input, reflen)
            }
            else if let PdfObject::Integer(length) = **object {
                take!(input, length)
            } else {
                error_code!(IResult::Error(ErrorKind::Custom(4)))
            }
        }
        else {
            error_code!(IResult::Error(ErrorKind::Custom(5)))
        }
    }
    else {
        error_code!(IResult::Error(ErrorKind::Custom(6)))
    }
}

pub fn stream_or_dictionary<'a>(input: &'a [u8], xref: &XRef, data: &'a [u8]) -> IResult<&'a [u8], PdfObject> {
    map!(input,
        do_parse!(
            dict: fs!(apply!(dictionary, xref, data)) >>
            stream: opt!(
                do_parse!(
                    tag!("stream") >>
                    alt!(tag!("\n") | tag!("\r\n")) >>
                    bytes: apply!(stream_bytes, &dict, xref, data) >>
                    alt!(tag!("\n") | tag!("\r\n") | tag!("\r")) >>
                    tag!("endstream") >>
                    (bytes)
                )
            ) >>
            (dict, stream)
        ),
        |(dict, opt_stream)| {
            if let Some(bytes) = opt_stream {
                PdfObject::Stream(Box::new(dict), bytes.to_vec())
            } else {
                dict
            }
        }
    )
}

named!(reference <PdfObject>,
    map!(
        do_parse!(
            number: fs!(digit) >>
            generation: fs!(digit) >>
            tag!("R") >>
            (number, generation)
        ),
        |(n, g)| {
            let number = i32::from_str(unsafe {
                from_utf8_unchecked(n)
            }).unwrap();
            let generation = i32::from_str(unsafe {
                from_utf8_unchecked(g)
            }).unwrap();
            PdfObject::Reference(number, generation)
        }
    )
);

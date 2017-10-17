extern crate nom;

use nom::{digit, hex_digit, IResult, ErrorKind, Needed};
use std::str::{FromStr, from_utf8, from_utf8_unchecked};
use std::collections::HashMap;
use std::fmt::Debug;

use super::XRef;

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
    Dictionary(HashMap<Box<[u8]>, PdfObject>),
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
            let res = object(&data[offset..], xref, data);
            if let IResult::Done(_, PdfObject::Indirect(_, _, o)) = res {
                return Some(*o)
            }
        }

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

    
pub fn object<'a>(input: &'a [u8], xref: &XRef, data: &'a [u8]) -> IResult<&'a [u8], PdfObject> {
    alt!(input,
        null | boolean | reference | apply!(indirect_object, xref, data) | real | integer | apply!(dictionary, xref, data) | hex_literal | string_literal | name_object | apply!(array, xref, data)
    )
}

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
            let number = i32::from_str(from_utf8(n).unwrap()).unwrap();
            let generation = i32::from_str(from_utf8(g).unwrap()).unwrap();
            PdfObject::Indirect(number, generation, Box::new(o))
        }
    )
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
    const HEX_DIGITS: &'static [u8] = b"0123456789ABCDEF";

    let c = HEX_DIGITS.iter().position(
        |&c| {
            assert!(is_hex_digit(c));
            c == if b'a' <= s && s <= b'f' {
                s - (b'a' - b'A')
            } else {
                s
            }
        }).unwrap();
    return c as u8;
}

fn hex_literal_digits(s: &[u8]) -> PdfObject {
    // TODO: see max string length limit
    let max_iter = if s.len() % 2 == 0 {
        s.len()
    } else {
        s.len() - 1
    };

    let mut result = Vec::with_capacity((max_iter + 1) / 2);
    let mut i = 0;
    while i < max_iter {
        let c1 = from_hex_char(s[i]);
        let c2 = from_hex_char(s[i+1]);
        result.push(c1*16u8 + c2);
        i += 2;
    }

    if s.len() % 2 != 0 {
        let c = from_hex_char(s[i]);
        result.push(16u8 * c);
    }

    return PdfObject::String(result);
}

named!(hex_literal <PdfObject>,
    map!(
        delimited!(
            char!('<'),
            hex_digit,
            char!('>')
        ),
        hex_literal_digits
    )
);

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
     (x < b'0' || x > b'9')
     && (x < b'a' || x > b'f')
     && (x < b'A' || x > b'F')
}

fn escape_name_object(s: &[u8]) -> Option<Vec<u8>> {
    let mut result = Vec::new();

    let mut hex = 0u8;
    let mut hex_count = 0u8;
    for c in s {
        if hex_count > 0 {
            if is_hex_digit(*c) {
                hex = hex*16u8 + from_hex_char(*c);
                hex_count -= 1;

                if hex_count == 0 {
                    result.push(hex);
                    hex = 0u8;
                }
            } else {
                return None;
            }
        }
        else if *c == b'#' {
            hex_count = 2;
        }
        else {
            result.push(*c);
        }
    }

    Some(result)
}

named!(name_object <PdfObject>,
    map_opt!(
        do_parse!(
            char!('/') >>
            res: take_while1!(is_regular) >>
            (res)
        ),
        |slice| {
            escape_name_object(slice).map(PdfObject::NameObject)
        }
    )
);

pub fn array<'a>(input: &'a [u8], xref: &XRef, data: &'a [u8]) -> IResult<&'a [u8], PdfObject> {
    map!(input,
        delimited!(
            fs!(char!('[')),
            many0!(
                fs!(apply!(object, xref, data))
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
   map!(input,
       delimited!(
           fs!(tag!("<<")),
           many1!(
               do_parse!(
                   key: fs!(name_object) >>
                   entry: fs!(apply!(object, xref, data)) >>
                   (key, entry)
               )
           ),
           tag!(">>")
       ),
       |vec| {
           let mut dict = HashMap::new();

           for (key, entry) in vec {
               if let PdfObject::NameObject(key) = key {
                   if entry != PdfObject::Null {
                       dict.insert(key.into_boxed_slice(), entry);
                   }
               }
           }
           PdfObject::Dictionary(dict)
       }
   )
}

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

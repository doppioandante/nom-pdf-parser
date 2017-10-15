#[macro_use]
extern crate nom;

use nom::{hex_digit, IResult, ErrorKind, Needed};
use std::str::{FromStr, from_utf8};

#[derive(Debug)]
enum PdfObject {
    Boolean(bool),
    Integer(i32), // TODO: see limits?
    Real(f64),
    String(Vec<u8>),
    NameObject(Vec<u8>),
    Array(Vec<PdfObject>),
    Dictionary(Vec<(PdfObject, PdfObject)>),
    Stream(Box<PdfObject>, Vec<u8>),
    Indirect(i32, i32, Box<PdfObject>), // TODO i32?
    Reference(i32, i32)
}

//impl PdfObject {
//    fn evaluate(&self) -> PdfObject {
//        if let PdfObject::Reference
//    

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
            //TODO: use a tag that works with bytes, should be faster
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
                nom::digit,
                |parsed_digits| {
                    // FIXME: is from_utf8 slow?
                    let mut value = i32::from_str(from_utf8(parsed_digits).unwrap()).unwrap();
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
            integral: opt!(nom::digit) >>
            char!('.') >>
            result: map!(
                opt!(nom::digit),
                |parsed_digits| {
                    let mut real_parsed = String::new();
                    // FIXME: is from_utf8 slow?
                    if let Some(c) = sign {
                        real_parsed.push(c);
                    }
                    if let Some(parsed) = integral {
                        real_parsed += from_utf8(parsed).unwrap();
                    }
                    real_parsed += ".";
                    if parsed_digits.is_some() {
                        real_parsed += from_utf8(parsed_digits.unwrap()).unwrap();
                    }
                    f64::from_str(real_parsed.as_str()).unwrap()
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
            hex_digit, // FIXME: why doesn't nom::hex_digit work?
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
                // FIXME: handle overflow
                octal = 8*octal + s[i] - b'0';
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

fn is_whitespace(c: u8) -> bool {
    match c {
        // TODO
        // FIXME: ] << >>
        b' ' | b'\n' | b']' | b'<' | b'>' => true,
        _ => false
    }
}

fn escape_name_object(s: &[u8]) -> Vec<u8> {
    let mut result = Vec::new();

    let mut hex = 0u8;
    let mut hex_count = 0u8;
    for c in s {
        if hex_count > 0 {
            // TODO: check wrong hex character :(
            hex = hex*16u8 + from_hex_char(*c);
            hex_count -= 1;

            if hex_count == 0 {
                result.push(hex);
                hex = 0u8;
            }
        }
        else if *c == b'#' {
            hex_count = 2;
        }
        else {
            result.push(*c);
        }
    }

    result
}

named!(name_object <PdfObject>,
    map!(
        do_parse!(
            char!('/') >>
            //FIXME: what if [\LOL] ? is_whitespace not enough?
            res: take_till1!(is_whitespace) >>
            (res)
        ),
        |slice| {
            PdfObject::NameObject(escape_name_object(slice))
        }
    )
);

named!(array <PdfObject>,
    // FIXME: not sure about whitespace handling
    map!(
        delimited!(
            ws!(char!('[')),
            separated_list!(
                nom::multispace,
                object
            ),
            ws!(char!(']'))
        ),
        PdfObject::Array
    )
);

named!(dictionary <PdfObject>,
   // FIXME: not sure about whitespace handling
   map!(
       delimited!(
           ws!(tag!("<<")),
           separated_list!(
               nom::multispace,
               // dict entry
               do_parse!(
                   key: ws!(name_object) >>
                   entry: object >>
                   (key, entry)
               )
           ),
           ws!(tag!(">>"))
       ),
       PdfObject::Dictionary
   )
);

// should probably be a macro
fn stream_bytes<'a>(dict: &PdfObject) -> Box<Fn(&'a [u8]) -> IResult<&'a [u8], &'a [u8]>> {
    if let PdfObject::Dictionary(ref vec) = *dict {
        let entry = vec.iter().find(|&entry| {
            if let (PdfObject::NameObject(ref key), _) = *entry {
                return &key[..] == b"Length";
            }
            false
        });

        if let Some(&(_, PdfObject::Integer(length))) = entry {
            Box::new(move |bytes| {
                take!(bytes, length)
            })
        } else {
            Box::new(|_| {
                error_code!(IResult::Error(ErrorKind::Custom(4)))
            })
        }
    }
    else {
        Box::new(|_| {
            error_code!(IResult::Error(ErrorKind::Custom(4)))
        })
    }
}

// workaround, thanks #nom channel
fn stream_bytes_helper<'a>(input: &'a [u8], dict: &PdfObject) -> IResult<&'a [u8], &'a [u8]> {
    let f = stream_bytes(&dict);
    f(input)
}

named!(stream_or_dictionary <PdfObject>,
    map!(
        do_parse!(
            dict: dictionary >>
            stream: opt!(
                do_parse!(
                    // dictionary eats previous space
                    tag!("stream") >>
                    alt!(tag!("\n") | tag!("\r\n")) >>
                    bytes: apply!(stream_bytes_helper, &dict) >>
                    //eol >> FIXME: \n or \r\n
                    ws!(tag!("endstream")) >>
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
);

use nom::digit;

named!(indirect_object <PdfObject>,
    map!(
        do_parse!(
            number: digit >>
            generation: ws!(digit) >>
            ws!(tag!("obj")) >>
            object: alt!(
                boolean | real | integer | stream_or_dictionary | hex_literal | string_literal | name_object | array
            ) >>
            tag!("endobj") >>
            (number, generation, object)
        ),
        |(n, g, object)| {
            let number = i32::from_str(from_utf8(n).unwrap()).unwrap();
            let generation = i32::from_str(from_utf8(g).unwrap()).unwrap();
            PdfObject::Indirect(number, generation, Box::new(object))
        }
    )
);

named!(reference <PdfObject>,
    map!(
        do_parse!(
            number: digit >>
            generation: ws!(digit) >>
            ws!(tag!("R")) >>
            (number, generation)
        ),
        |(n, g)| {
            let number = i32::from_str(from_utf8(n).unwrap()).unwrap();
            let generation = i32::from_str(from_utf8(g).unwrap()).unwrap();
            PdfObject::Reference(number, generation)
        }
    )
);

named!(object <PdfObject>,
    alt!(
        boolean | reference | indirect_object | real | integer | dictionary | hex_literal | string_literal | name_object | array
    )
);


fn main() {
    let data = include_bytes!("parse_data");
    let res = object(data);
    if let IResult::Done(_, PdfObject::String(vec)) = res {
        println!("{}", from_utf8(vec.as_slice()).unwrap());
    } else if res.is_done() {
        println!("{:?}", res);
    } else {
        println!("Error: {:?}", res);
    }
}

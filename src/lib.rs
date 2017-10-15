#[macro_use]
extern crate nom;

use std::collections::HashMap;

pub mod parser;

struct XRefEntry {
    offset: u32, // TODO limits
    generation: u32,
    in_use: bool
}

pub struct XRef {
    table: HashMap<u32, XRefEntry>
}

impl XRef {
    pub fn new() -> Self {
        return XRef{
            table: HashMap::new()
        }
    }

    pub fn add_entry(&mut self, number: u32, offset: u32, generation: u32, in_use: bool) {
        self.table.insert(number,
            XRefEntry{
                offset,
                generation,
                in_use
            });
    }

    pub fn get_offset(&self, number: u32) -> u32 {
        self.table.get(&number).unwrap().offset
    }
        
}




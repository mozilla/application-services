/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Utility to format and print text tables to console.
//!
//! Has an API similar to prettytable-rs, but simplified. The goal is to provide enough
//! functionality to format tables, but with less dependencies.

use std::{cmp::max, fmt::Display};

use unicode_width::UnicodeWidthStr;

#[macro_export]
macro_rules! row {
    ($($value:expr),* $(,)?) => {
        $crate::Row::new(vec![$(Cell::new($value.to_string())),*])
    };
}

pub struct Cell {
    text: String,
    alignment: Alignment,
}

#[derive(Default)]
pub struct Row {
    cells: Vec<Cell>,
}

#[derive(Default)]
pub struct Table {
    rows: Vec<Row>,
}

#[derive(PartialEq, Eq)]
pub enum Alignment {
    Left,
    Center,
    Right,
}

impl Cell {
    pub fn new(value: impl Display) -> Self {
        Self {
            text: value.to_string(),
            alignment: Alignment::Left,
        }
    }

    pub fn align_right(self) -> Self {
        Self {
            alignment: Alignment::Right,
            ..self
        }
    }

    pub fn align_center(self) -> Self {
        Self {
            alignment: Alignment::Center,
            ..self
        }
    }

    pub fn align_left(self) -> Self {
        Self {
            alignment: Alignment::Left,
            ..self
        }
    }
}

impl Row {
    pub fn new(cells: Vec<Cell>) -> Self {
        Self { cells }
    }

    pub fn empty() -> Self {
        Self::default()
    }

    pub fn add_cell(mut self, cell_data: impl Display) -> Self {
        self.cells.push(Cell::new(cell_data));
        self
    }

    pub fn add_center(mut self, cell_data: impl Display) -> Self {
        self.cells.push(Cell::new(cell_data).align_center());
        self
    }

    pub fn add_right(mut self, cell_data: impl Display) -> Self {
        self.cells.push(Cell::new(cell_data).align_right());
        self
    }
}

impl Table {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_row(&mut self, row: Row) {
        self.rows.push(row);
    }

    pub fn printstd(&self) {
        let mut row_widths = vec![];
        for row in self.rows.iter() {
            for (i, cell) in row.cells.iter().enumerate() {
                let width = UnicodeWidthStr::width(cell.text.as_str());
                if i >= row_widths.len() {
                    row_widths.push(width);
                } else {
                    row_widths[i] = max(row_widths[i], width);
                }
            }
        }
        let row_separator = self.make_row_separator(&row_widths);
        println!("{row_separator}");
        for row in self.rows.iter() {
            for (cell, row_width) in row.cells.iter().zip(row_widths.iter()) {
                print!("|");
                print!("{}", self.format_cell(cell, *row_width));
            }
            println!("|");
            println!("{row_separator}");
        }
    }

    fn make_row_separator(&self, row_widths: &[usize]) -> String {
        let mut text = String::from("+");
        for width in row_widths {
            text.push_str(&"-".repeat(width + 2));
            text.push('+');
        }
        text
    }

    fn format_cell(&self, cell: &Cell, row_width: usize) -> String {
        let text = &cell.text;
        match cell.alignment {
            Alignment::Left => {
                format!(" {text}{} ", " ".repeat(row_width - text.len()))
            }
            Alignment::Center => {
                let left_pad = (row_width - text.len()) / 2;
                format!(
                    " {}{text}{} ",
                    " ".repeat(left_pad),
                    " ".repeat(row_width - text.len() - left_pad)
                )
            }
            Alignment::Right => {
                format!(" {}{text} ", " ".repeat(row_width - text.len()))
            }
        }
    }
}

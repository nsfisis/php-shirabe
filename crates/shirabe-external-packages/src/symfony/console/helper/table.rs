//! ref: composer/vendor/symfony/console/Helper/Table.php

use crate::composer::pcre::preg::Preg;
use crate::symfony::console::exception::invalid_argument_exception::InvalidArgumentException;
use crate::symfony::console::exception::runtime_exception::RuntimeException;
use crate::symfony::console::formatter::output_formatter::OutputFormatter;
use crate::symfony::console::formatter::wrappable_output_formatter_interface::WrappableOutputFormatterInterface;
use crate::symfony::console::helper::helper::Helper;
use crate::symfony::console::helper::table_cell::{TableCell, TableCellOption};
use crate::symfony::console::helper::table_cell_style::TableCellStyle;
use crate::symfony::console::helper::table_rows::TableRows;
use crate::symfony::console::helper::table_separator::TableSeparator;
use crate::symfony::console::helper::table_style::TableStyle;
use crate::symfony::console::output::console_section_output::ConsoleSectionOutput;
use crate::symfony::console::output::output_interface::OutputInterface;
use indexmap::IndexMap;
use shirabe_php_shim::AsAny;
use shirabe_php_shim::PhpMixed;
use std::cell::RefCell;
use std::rc::Rc;

/// A single cell within a table row.
///
/// PHP types a cell as `TableCell|string|int|null` (scalars are stringified). Because
/// `TableSeparator extends TableCell`, a separator can in principle appear as a cell, so it has its
/// own variant; `is_table_cell` reports `true` for it, mirroring `instanceof TableCell`.
#[derive(Debug, Clone)]
pub enum Cell {
    Null,
    Value(String),
    Cell(TableCell),
    Separator(TableSeparator),
}

impl Cell {
    /// PHP `$cell instanceof TableCell` (true for `TableSeparator`, which extends `TableCell`).
    fn is_table_cell(&self) -> bool {
        matches!(self, Cell::Cell(_) | Cell::Separator(_))
    }

    /// PHP `$cell instanceof TableSeparator`.
    fn is_table_separator(&self) -> bool {
        matches!(self, Cell::Separator(_))
    }

    fn colspan(&self) -> i64 {
        match self {
            Cell::Cell(c) => c.get_colspan(),
            Cell::Separator(s) => s.get_colspan(),
            _ => 1,
        }
    }

    fn rowspan(&self) -> i64 {
        match self {
            Cell::Cell(c) => c.get_rowspan(),
            Cell::Separator(s) => s.get_rowspan(),
            _ => 1,
        }
    }

    fn style(&self) -> Option<Rc<TableCellStyle>> {
        match self {
            Cell::Cell(c) => c.get_style(),
            Cell::Separator(s) => s.get_style(),
            _ => None,
        }
    }

    /// PHP `(string) $cell`.
    fn to_php_string(&self) -> String {
        match self {
            Cell::Null | Cell::Separator(_) => String::new(),
            Cell::Value(s) => s.clone(),
            Cell::Cell(c) => c.to_string(),
        }
    }

    /// PHP truthiness / `!empty($cell)` for a cell value: only "" and "0" (and null) are falsy;
    /// any object is truthy.
    fn is_truthy(&self) -> bool {
        match self {
            Cell::Null => false,
            Cell::Value(s) => !s.is_empty() && s != "0",
            Cell::Cell(_) | Cell::Separator(_) => true,
        }
    }

    fn is_null(&self) -> bool {
        matches!(self, Cell::Null)
    }
}

impl From<String> for Cell {
    fn from(value: String) -> Self {
        Cell::Value(value)
    }
}

impl From<&str> for Cell {
    fn from(value: &str) -> Self {
        Cell::Value(value.to_string())
    }
}

impl From<TableCell> for Cell {
    fn from(value: TableCell) -> Self {
        Cell::Cell(value)
    }
}

/// Bridges PHP-typed cell values built elsewhere in the codebase (where rows are still assembled as
/// `PhpMixed`) onto the typed cell. A cell is a scalar or null; collections stringify as PHP would.
impl From<PhpMixed> for Cell {
    fn from(value: PhpMixed) -> Self {
        match value {
            PhpMixed::Null => Cell::Null,
            other => Cell::Value(shirabe_php_shim::to_string(&other)),
        }
    }
}

/// A table row.
///
/// PHP types a row as `array<int, Cell>|TableSeparator`. The header/body boundary that PHP creates
/// as a fresh `TableSeparator` and recognizes by object identity (`$divider === $row`) is modeled
/// here by its own [`Row::HeaderDivider`] variant rather than by reference identity: it counts as a
/// separator for layout (`is_table_separator`), but unlike a user-supplied [`Row::Separator`] it
/// renders no separator line — it only flips the header/first-row state during rendering.
#[derive(Debug, Clone)]
pub enum Row {
    HeaderDivider,
    Separator(TableSeparator),
    Cells(Vec<Cell>),
}

impl Row {
    /// PHP `$row instanceof TableSeparator` (also true for the internal header/body divider).
    fn is_table_separator(&self) -> bool {
        matches!(self, Row::HeaderDivider | Row::Separator(_))
    }

    /// PHP `$divider === $row`: identifies the internal header/body boundary marker.
    fn is_divider(&self) -> bool {
        matches!(self, Row::HeaderDivider)
    }

    /// The row's cells, or an empty list for a separator (PHP `foreach` over a separator object,
    /// which exposes no public properties, yields nothing).
    fn cells(&self) -> Vec<Cell> {
        match self {
            Row::Cells(cells) => cells.clone(),
            _ => Vec::new(),
        }
    }

    /// PHP `!$row`: an empty cell array is falsy; a separator is an object and thus truthy.
    fn is_truthy(&self) -> bool {
        match self {
            Row::Cells(cells) => !cells.is_empty(),
            _ => true,
        }
    }

    /// PHP `isset($row[$i])`: `false` for a missing or null cell.
    fn get_cell_isset(&self, index: i64) -> Option<Cell> {
        match self {
            Row::Cells(cells) => match cells.get(index as usize) {
                Some(Cell::Null) | None => None,
                Some(cell) => Some(cell.clone()),
            },
            _ => None,
        }
    }
}

impl From<Vec<Cell>> for Row {
    fn from(cells: Vec<Cell>) -> Self {
        Row::Cells(cells)
    }
}

/// Bridges PHP-typed rows built elsewhere (still assembled as `PhpMixed`) onto the typed row.
impl From<PhpMixed> for Row {
    fn from(value: PhpMixed) -> Self {
        match value {
            PhpMixed::List(items) => Row::Cells(items.into_iter().map(Cell::from).collect()),
            PhpMixed::Array(map) => Row::Cells(map.into_values().map(Cell::from).collect()),
            PhpMixed::Null => Row::Cells(Vec::new()),
            other => Row::Cells(vec![Cell::from(other)]),
        }
    }
}

/// A table or column style argument. PHP types it as `string|TableStyle`: either the name of a
/// registered style or a `TableStyle` instance.
#[derive(Debug, Clone)]
pub enum StyleName {
    Name(String),
    Style(TableStyle),
}

impl From<&str> for StyleName {
    fn from(name: &str) -> Self {
        StyleName::Name(name.to_string())
    }
}

impl From<String> for StyleName {
    fn from(name: String) -> Self {
        StyleName::Name(name)
    }
}

impl From<TableStyle> for StyleName {
    fn from(style: TableStyle) -> Self {
        StyleName::Style(style)
    }
}

/// PHP `$row[$index] = $value`: assigns at the integer key, growing the (dense) row with null
/// cells to reach `index` when necessary.
fn set_cell(row: &mut Vec<Cell>, index: i64, value: Cell) {
    let index = index as usize;
    while row.len() <= index {
        row.push(Cell::Null);
    }
    row[index] = value;
}

/// Provides helpers to display a table.
#[derive(Debug)]
pub struct Table {
    header_title: Option<String>,
    footer_title: Option<String>,

    /// Table headers.
    headers: Vec<Row>,

    /// Table rows.
    rows: Vec<Row>,
    horizontal: bool,

    /// Column widths cache.
    effective_column_widths: IndexMap<i64, i64>,

    /// Number of columns cache.
    number_of_columns: Option<i64>,

    output: Rc<RefCell<dyn OutputInterface>>,

    style: TableStyle,

    column_styles: IndexMap<i64, TableStyle>,

    /// User set column widths.
    column_widths: IndexMap<i64, i64>,
    column_max_widths: IndexMap<i64, i64>,

    rendered: bool,
}

const SEPARATOR_TOP: i64 = 0;
const SEPARATOR_TOP_BOTTOM: i64 = 1;
const SEPARATOR_MID: i64 = 2;
const SEPARATOR_BOTTOM: i64 = 3;
const BORDER_OUTSIDE: i64 = 0;
const BORDER_INSIDE: i64 = 1;

/// Global style definitions, lazily initialized.
///
/// In PHP this is `private static $styles`. Here it is a process-global cache.
fn styles() -> &'static std::sync::Mutex<Option<IndexMap<String, TableStyle>>> {
    static STYLES: std::sync::Mutex<Option<IndexMap<String, TableStyle>>> =
        std::sync::Mutex::new(None);
    &STYLES
}

impl Table {
    pub fn new(output: Rc<RefCell<dyn OutputInterface>>) -> Self {
        let mut styles_guard = styles().lock().unwrap();
        if styles_guard.is_none() {
            *styles_guard = Some(Self::init_styles());
        }
        drop(styles_guard);

        let mut this = Self {
            header_title: None,
            footer_title: None,
            headers: Vec::new(),
            rows: Vec::new(),
            horizontal: false,
            effective_column_widths: IndexMap::new(),
            number_of_columns: None,
            output,
            style: TableStyle::default(),
            column_styles: IndexMap::new(),
            column_widths: IndexMap::new(),
            column_max_widths: IndexMap::new(),
            rendered: false,
        };

        this.set_style(StyleName::from("default"));

        this
    }

    /// Sets a style definition.
    pub fn set_style_definition(name: String, style: TableStyle) {
        let mut styles_guard = styles().lock().unwrap();
        if styles_guard.is_none() {
            *styles_guard = Some(Self::init_styles());
        }

        styles_guard.as_mut().unwrap().insert(name, style);
    }

    /// Gets a style definition by name.
    pub fn get_style_definition(
        name: String,
    ) -> anyhow::Result<Result<TableStyle, InvalidArgumentException>> {
        let mut styles_guard = styles().lock().unwrap();
        if styles_guard.is_none() {
            *styles_guard = Some(Self::init_styles());
        }

        if let Some(style) = styles_guard.as_ref().unwrap().get(&name) {
            return Ok(Ok(style.clone()));
        }

        Ok(Err(InvalidArgumentException(
            shirabe_php_shim::InvalidArgumentException {
                message: format!("Style \"{}\" is not defined.", name),
                code: 0,
            },
        )))
    }

    /// Sets table style.
    ///
    /// `$name` is the style name or a TableStyle instance.
    pub fn set_style(
        &mut self,
        name: StyleName,
    ) -> anyhow::Result<Result<&mut Self, InvalidArgumentException>> {
        match self.resolve_style(name)? {
            Ok(style) => {
                self.style = style;
                Ok(Ok(self))
            }
            Err(e) => Ok(Err(e)),
        }
    }

    /// Gets the current table style.
    pub fn get_style(&self) -> &TableStyle {
        &self.style
    }

    /// Sets table column style.
    ///
    /// `$name` is the style name or a TableStyle instance.
    pub fn set_column_style(
        &mut self,
        column_index: i64,
        name: StyleName,
    ) -> anyhow::Result<Result<&mut Self, InvalidArgumentException>> {
        match self.resolve_style(name)? {
            Ok(style) => {
                self.column_styles.insert(column_index, style);
                Ok(Ok(self))
            }
            Err(e) => Ok(Err(e)),
        }
    }

    /// Gets the current style for a column.
    ///
    /// If style was not set, it returns the global table style.
    pub fn get_column_style(&self, column_index: i64) -> &TableStyle {
        self.column_styles
            .get(&column_index)
            .unwrap_or_else(|| self.get_style())
    }

    /// Sets the minimum width of a column.
    pub fn set_column_width(&mut self, column_index: i64, width: i64) -> &mut Self {
        self.column_widths.insert(column_index, width);

        self
    }

    /// Sets the minimum width of all columns.
    pub fn set_column_widths(&mut self, widths: Vec<i64>) -> &mut Self {
        self.column_widths = IndexMap::new();
        for (index, width) in widths.into_iter().enumerate() {
            self.set_column_width(index as i64, width);
        }

        self
    }

    /// Sets the maximum width of a column.
    ///
    /// Any cell within this column which contents exceeds the specified width will be wrapped into
    /// multiple lines, while formatted strings are preserved.
    pub fn set_column_max_width(&mut self, column_index: i64, width: i64) -> &mut Self {
        if !Self::formatter_is_wrappable(&self.output) {
            // PHP throws \LogicException here. This represents a programming error: the caller must
            // supply a WrappableOutputFormatterInterface before setting a maximum column width.
            panic!(
                "Setting a maximum column width is only supported when using a \"{}\" formatter, got \"{}\".",
                "Symfony\\Component\\Console\\Formatter\\WrappableOutputFormatterInterface",
                shirabe_php_shim::get_debug_type(&PhpMixed::from(()))
            );
        }

        self.column_max_widths.insert(column_index, width);

        self
    }

    pub fn set_headers(&mut self, headers: Vec<Cell>) -> &mut Self {
        // PHP wraps a flat list of cells into a single header row. (Multi-row headers, which PHP
        // also accepts, are not used in this codebase and are not modeled by the typed API.)
        self.headers = if headers.is_empty() {
            Vec::new()
        } else {
            vec![Row::Cells(headers)]
        };

        self
    }

    pub fn set_rows(&mut self, rows: Vec<Row>) -> &mut Self {
        self.rows = Vec::new();

        self.add_rows(rows)
    }

    pub fn add_rows(&mut self, rows: Vec<Row>) -> &mut Self {
        for row in rows {
            self.add_row(row);
        }

        self
    }

    pub fn add_row(&mut self, row: Row) -> &mut Self {
        // PHP `array_values($row)` reindexing is the identity on a positional cell vector, and the
        // "row must be an array or a TableSeparator" check is now guaranteed by the type.
        self.rows.push(row);

        self
    }

    /// Adds a row to the table, and re-renders the table.
    pub fn append_row(
        &mut self,
        row: Row,
    ) -> anyhow::Result<Result<&mut Self, RuntimeException>> {
        if !Self::output_is_console_section(&self.output) {
            return Ok(Err(RuntimeException(shirabe_php_shim::RuntimeException {
                message: format!(
                    "Output should be an instance of \"{}\" when calling \"{}\".",
                    "Symfony\\Component\\Console\\Output\\ConsoleSectionOutput",
                    "Symfony\\Component\\Console\\Helper\\Table::appendRow",
                ),
                code: 0,
            })));
        }

        if self.rendered {
            // TODO(phase-b): downcast output to ConsoleSectionOutput to call clear().
            let _ = ConsoleSectionOutput::clear;
            let row_count = self.calculate_row_count();
            let _ = row_count;
            todo!()
        }

        self.add_row(row);
        self.render();

        Ok(Ok(self))
    }

    pub fn set_row(&mut self, column: i64, row: Vec<Cell>) -> &mut Self {
        // PHP indexes $this->rows by arbitrary key; sparse assignment over a positional Vec is not
        // modeled and has no callers.
        let _ = (column, row);
        // TODO(phase-d): sparse `$this->rows[$column] = $row` over a positional row vector.
        todo!()
    }

    pub fn set_header_title(&mut self, title: Option<String>) -> &mut Self {
        self.header_title = title;

        self
    }

    pub fn set_footer_title(&mut self, title: Option<String>) -> &mut Self {
        self.footer_title = title;

        self
    }

    pub fn set_horizontal(&mut self, horizontal: bool) -> &mut Self {
        self.horizontal = horizontal;

        self
    }

    /// Renders table to output.
    pub fn render(&mut self) {
        let rows: Vec<Row>;
        if self.horizontal {
            let mut horizontal_rows: IndexMap<i64, Vec<Cell>> = IndexMap::new();
            let header0 = self
                .headers
                .first()
                .map(|h| h.cells())
                .unwrap_or_default();
            for (i, header) in header0.into_iter().enumerate() {
                let i = i as i64;
                horizontal_rows.insert(i, vec![header]);
                for row in &self.rows {
                    if row.is_table_separator() {
                        continue;
                    }
                    if let Some(cell) = row.get_cell_isset(i) {
                        let entry = horizontal_rows.get_mut(&i).unwrap();
                        entry.push(cell);
                    } else {
                        let first = horizontal_rows.get(&i).unwrap().first().cloned();
                        let is_title_noop = match first {
                            Some(ref c) if c.is_table_cell() => c.colspan() >= 2,
                            _ => false,
                        };
                        if is_title_noop {
                            // Noop, there is a "title"
                        } else {
                            let entry = horizontal_rows.get_mut(&i).unwrap();
                            entry.push(Cell::Null);
                        }
                    }
                }
            }
            rows = horizontal_rows.into_values().map(Row::Cells).collect();
        } else {
            let mut merged = self.headers.clone();
            merged.push(Row::HeaderDivider);
            merged.extend(self.rows.clone());
            rows = merged;
        }

        self.calculate_number_of_columns(&rows);

        let row_groups = self.build_table_rows(rows);
        self.calculate_columns_width(&row_groups);

        let mut is_header = !self.horizontal;
        let mut is_first_row = self.horizontal;
        let mut has_title =
            self.header_title.is_some() && !self.header_title.as_deref().unwrap_or("").is_empty();

        for row_group in &row_groups {
            let mut is_header_separator_rendered = false;

            for row in row_group {
                if row.is_divider() {
                    is_header = false;
                    is_first_row = true;

                    continue;
                }

                if row.is_table_separator() {
                    self.render_row_separator(SEPARATOR_MID, None, None);

                    continue;
                }

                if !row.is_truthy() {
                    continue;
                }

                if is_header && !is_header_separator_rendered {
                    self.render_row_separator(
                        if is_header {
                            SEPARATOR_TOP
                        } else {
                            SEPARATOR_TOP_BOTTOM
                        },
                        if has_title {
                            self.header_title.clone()
                        } else {
                            None
                        },
                        if has_title {
                            Some(self.style.get_header_title_format())
                        } else {
                            None
                        },
                    );
                    has_title = false;
                    is_header_separator_rendered = true;
                }

                if is_first_row {
                    self.render_row_separator(
                        if is_header {
                            SEPARATOR_TOP
                        } else {
                            SEPARATOR_TOP_BOTTOM
                        },
                        if has_title {
                            self.header_title.clone()
                        } else {
                            None
                        },
                        if has_title {
                            Some(self.style.get_header_title_format())
                        } else {
                            None
                        },
                    );
                    is_first_row = false;
                    has_title = false;
                }

                if self.horizontal {
                    self.render_row(
                        row.cells(),
                        self.style.get_cell_row_format(),
                        Some(self.style.get_cell_header_format()),
                    );
                } else {
                    self.render_row(
                        row.cells(),
                        if is_header {
                            self.style.get_cell_header_format()
                        } else {
                            self.style.get_cell_row_format()
                        },
                        None,
                    );
                }
            }
        }
        self.render_row_separator(
            SEPARATOR_BOTTOM,
            self.footer_title.clone(),
            Some(self.style.get_footer_title_format()),
        );

        self.cleanup();
        self.rendered = true;
    }

    /// Renders horizontal header separator.
    fn render_row_separator(
        &self,
        r#type: i64,
        title: Option<String>,
        title_format: Option<String>,
    ) {
        let count = match self.number_of_columns {
            Some(0) | None => return,
            Some(c) => c,
        };

        let borders = self.style.get_border_chars();
        if borders[0].is_empty()
            && borders[2].is_empty()
            && self.style.get_crossing_char().is_empty()
        {
            return;
        }

        let crossings = self.style.get_crossing_chars();
        let (horizontal, left_char, mid_char, right_char) = if SEPARATOR_MID == r#type {
            (
                borders[2].clone(),
                crossings[8].clone(),
                crossings[0].clone(),
                crossings[4].clone(),
            )
        } else if SEPARATOR_TOP == r#type {
            (
                borders[0].clone(),
                crossings[1].clone(),
                crossings[2].clone(),
                crossings[3].clone(),
            )
        } else if SEPARATOR_TOP_BOTTOM == r#type {
            (
                borders[0].clone(),
                crossings[9].clone(),
                crossings[10].clone(),
                crossings[11].clone(),
            )
        } else {
            (
                borders[0].clone(),
                crossings[7].clone(),
                crossings[6].clone(),
                crossings[5].clone(),
            )
        };

        let mut markup = left_char;
        let mut column = 0;
        while column < count {
            markup.push_str(&shirabe_php_shim::str_repeat(
                &horizontal,
                self.effective_column_widths[&column] as usize,
            ));
            markup.push_str(if column == count - 1 {
                &right_char
            } else {
                &mid_char
            });
            column += 1;
        }

        if let Some(title) = title {
            let title_format = title_format.unwrap();
            let formatted_title =
                shirabe_php_shim::sprintf(&title_format, &[PhpMixed::from(title.clone())]);
            let mut formatted_title = formatted_title;
            let mut title_length = Helper::width(&self.remove_decoration(&formatted_title));
            let markup_length = Helper::width(&markup);
            let limit = markup_length - 4;
            if title_length > limit {
                title_length = limit;
                let format_length = Helper::width(&self.remove_decoration(
                    &shirabe_php_shim::sprintf(&title_format, &[PhpMixed::from("")]),
                ));
                formatted_title = shirabe_php_shim::sprintf(
                    &title_format,
                    &[PhpMixed::from(format!(
                        "{}...",
                        Helper::substr(&title, 0, Some(limit - format_length - 3))
                    ))],
                );
            }

            let title_start = (markup_length - title_length) / 2;
            if shirabe_php_shim::mb_detect_encoding(&markup, None, true).is_none() {
                markup = shirabe_php_shim::substr_replace(
                    &markup,
                    &formatted_title,
                    title_start as usize,
                    title_length as usize,
                );
            } else {
                markup = format!(
                    "{}{}{}",
                    shirabe_php_shim::mb_substr(&markup, 0, Some(title_start), None),
                    formatted_title,
                    shirabe_php_shim::mb_substr(&markup, title_start + title_length, None, None),
                );
            }
        }

        self.output.borrow().writeln(
            &[shirabe_php_shim::sprintf(
                &self.style.get_border_format(),
                &[PhpMixed::from(markup)],
            )],
            crate::symfony::console::output::output_interface::OUTPUT_NORMAL,
        );
    }

    /// Renders vertical column separator.
    fn render_column_separator(&self, r#type: i64) -> String {
        let borders = self.style.get_border_chars();

        shirabe_php_shim::sprintf(
            &self.style.get_border_format(),
            &[PhpMixed::from(if BORDER_OUTSIDE == r#type {
                borders[1].clone()
            } else {
                borders[3].clone()
            })],
        )
    }

    /// Renders table row.
    fn render_row(
        &self,
        row: Vec<Cell>,
        cell_format: String,
        first_cell_format: Option<String>,
    ) {
        let mut row_content = self.render_column_separator(BORDER_OUTSIDE);
        let columns = self.get_row_columns(&row);
        let last = columns.len() as i64 - 1;
        for (i, column) in columns.into_iter().enumerate() {
            let i = i as i64;
            if first_cell_format.is_some() && 0 == i {
                row_content.push_str(&self.render_cell(
                    &row,
                    column,
                    first_cell_format.clone().unwrap(),
                ));
            } else {
                row_content.push_str(&self.render_cell(&row, column, cell_format.clone()));
            }
            row_content.push_str(&self.render_column_separator(if last == i {
                BORDER_OUTSIDE
            } else {
                BORDER_INSIDE
            }));
        }
        self.output.borrow().writeln(
            &[row_content],
            crate::symfony::console::output::output_interface::OUTPUT_NORMAL,
        );
    }

    /// Renders table cell with padding.
    fn render_cell(&self, row: &[Cell], column: i64, cell_format: String) -> String {
        let cell = row
            .get(column as usize)
            .cloned()
            .unwrap_or(Cell::Value(String::new()));
        let mut width = self.effective_column_widths[&column];
        if cell.is_table_cell() && cell.colspan() > 1 {
            // add the width of the following columns(numbers of colspan).
            for next_column in (column + 1)..=(column + cell.colspan() - 1) {
                width +=
                    self.get_column_separator_width() + self.effective_column_widths[&next_column];
            }
        }

        // str_pad won't work properly with multi-byte strings, we need to fix the padding
        let cell_str = cell.to_php_string();
        if let Some(encoding) = shirabe_php_shim::mb_detect_encoding(&cell_str, None, true) {
            width += shirabe_php_shim::strlen(&cell_str)
                - shirabe_php_shim::mb_strwidth(&cell_str, Some(&encoding));
        }

        let style = self.get_column_style(column);

        if cell.is_table_separator() {
            return shirabe_php_shim::sprintf(
                &style.get_border_format(),
                &[PhpMixed::from(shirabe_php_shim::str_repeat(
                    &style.get_border_chars()[2],
                    width as usize,
                ))],
            );
        }

        width += Helper::length(&cell_str) - Helper::length(&self.remove_decoration(&cell_str));
        let mut content = shirabe_php_shim::sprintf(
            &style.get_cell_row_content_format(),
            &[PhpMixed::from(cell_str.clone())],
        );

        let mut cell_format = cell_format;
        let mut pad_type = style.get_pad_type();
        if cell.is_table_cell() && cell.style().is_some() {
            let is_not_styled_by_tag = !Preg::is_match(
                "/^<(\\w+|(\\w+=[\\w,]+;?)*)>.+<\\/(\\w+|(\\w+=\\w+;?)*)?>$/",
                &cell_str,
            );
            if is_not_styled_by_tag {
                let cell_style = cell.style().unwrap();
                match cell_style.get_cell_format() {
                    Some(fmt) => cell_format = fmt,
                    None => {
                        let tag = shirabe_php_shim::http_build_query_mixed(
                            &cell_style.get_tag_options(),
                            "",
                            ";",
                        );
                        cell_format = format!("<{}>%s</>", tag);
                    }
                }

                if shirabe_php_shim::strstr(&content, "</>").is_some() {
                    content = shirabe_php_shim::str_replace("</>", "", &content);
                    width -= 3;
                }
                if shirabe_php_shim::strstr(&content, "<fg=default;bg=default>").is_some() {
                    content =
                        shirabe_php_shim::str_replace("<fg=default;bg=default>", "", &content);
                    width -= shirabe_php_shim::strlen("<fg=default;bg=default>");
                }
            }

            pad_type = cell.style().unwrap().get_pad_by_align();
        }

        shirabe_php_shim::sprintf(
            &cell_format,
            &[PhpMixed::from(shirabe_php_shim::str_pad(
                &content,
                width as usize,
                &style.get_padding_char(),
                pad_type,
            ))],
        )
    }

    /// Calculate number of columns for this table.
    fn calculate_number_of_columns(&mut self, rows: &[Row]) {
        let mut columns = vec![0i64];
        for row in rows {
            if row.is_table_separator() {
                continue;
            }

            columns.push(self.get_number_of_columns(&row.cells()));
        }

        self.number_of_columns = Some(*columns.iter().max().unwrap());
    }

    fn build_table_rows(&mut self, rows: Vec<Row>) -> TableRows {
        let mut rows = rows;
        let mut unmerged_rows: IndexMap<i64, IndexMap<i64, Vec<Cell>>> = IndexMap::new();
        let mut row_key = 0i64;
        while row_key < rows.len() as i64 {
            rows = self.fill_next_rows(rows, row_key);

            // Remove any new line breaks and replace it with a new line
            let current = rows[row_key as usize].cells();
            for (column, cell) in current.iter().enumerate() {
                let column = column as i64;
                let mut cell = cell.clone();
                let colspan = if cell.is_table_cell() {
                    cell.colspan()
                } else {
                    1
                };

                if self.column_max_widths.contains_key(&column)
                    && Helper::width(&self.remove_decoration(&cell.to_php_string()))
                        > self.column_max_widths[&column]
                {
                    // TODO(phase-b): formatAndWrap requires a WrappableOutputFormatterInterface;
                    // downcasting dyn OutputFormatterInterface to it needs concrete knowledge.
                    let _ = colspan;
                    let wrapped: Option<String> = todo!();
                    cell = Cell::Value(wrapped.unwrap_or_default());
                }
                let cell_str = cell.to_php_string();
                if shirabe_php_shim::strstr(&cell_str, "\n").is_none() {
                    continue;
                }
                let eol = if shirabe_php_shim::str_contains(&cell_str, "\r\n") {
                    "\r\n"
                } else {
                    "\n"
                };
                let escaped = shirabe_php_shim::implode(
                    eol,
                    &shirabe_php_shim::explode(eol, &cell_str)
                        .iter()
                        .map(|line| OutputFormatter::escape_trailing_backslash(line))
                        .collect::<Vec<_>>(),
                );
                cell = if cell.is_table_cell() {
                    Cell::Cell(TableCell::new2(
                        &escaped,
                        Self::table_cell_options_colspan(cell.colspan()),
                    ))
                } else {
                    Cell::Value(escaped.clone())
                };
                let lines = shirabe_php_shim::explode(
                    eol,
                    &shirabe_php_shim::str_replace(
                        eol,
                        &format!("<fg=default;bg=default></>{}", eol),
                        &cell.to_php_string(),
                    ),
                );
                for (line_key, line) in lines.into_iter().enumerate() {
                    let line_key = line_key as i64;
                    let mut line = Cell::Value(line);
                    if colspan > 1 {
                        line = Cell::Cell(TableCell::new2(
                            &line.to_php_string(),
                            Self::table_cell_options_colspan(colspan),
                        ));
                    }
                    if 0 == line_key {
                        let mut r = rows[row_key as usize].cells();
                        set_cell(&mut r, column, line);
                        rows[row_key as usize] = Row::Cells(r);
                    } else {
                        if !unmerged_rows.contains_key(&row_key)
                            || !unmerged_rows[&row_key].contains_key(&line_key)
                        {
                            let copied = self.copy_row(&rows, row_key);
                            unmerged_rows
                                .entry(row_key)
                                .or_default()
                                .insert(line_key, copied);
                        }
                        let target = unmerged_rows
                            .get_mut(&row_key)
                            .unwrap()
                            .get_mut(&line_key)
                            .unwrap();
                        set_cell(target, column, line);
                    }
                }
            }
            row_key += 1;
        }

        // PHP returns a TableRows wrapping a generator that lazily yields row groups.
        // The generator borrows $this to call fillCells(). In Phase A we precompute the
        // row groups eagerly to preserve behavior, then hand them to TableRows.
        let mut row_groups: Vec<Vec<Row>> = Vec::new();
        for (row_key, row) in rows.into_iter().enumerate() {
            let row_key = row_key as i64;
            let mut row_group: Vec<Row> = vec![if row.is_table_separator() {
                row
            } else {
                Row::Cells(self.fill_cells(row.cells()))
            }];

            if let Some(extra) = unmerged_rows.get(&row_key) {
                for r in extra.values() {
                    row_group.push(Row::Cells(self.fill_cells(r.clone())));
                }
            }
            row_groups.push(row_group);
        }

        TableRows::from_row_groups(row_groups)
    }

    fn calculate_row_count(&mut self) -> i64 {
        let mut merged = self.headers.clone();
        merged.push(Row::Separator(TableSeparator::new()));
        merged.extend(self.rows.clone());
        let mut number_of_rows = self.build_table_rows(merged).into_row_groups().len() as i64;

        if !self.headers.is_empty() {
            number_of_rows += 1; // Add row for header separator
        }

        if !self.rows.is_empty() {
            number_of_rows += 1; // Add row for footer separator
        }

        number_of_rows
    }

    /// fill rows that contains rowspan > 1.
    fn fill_next_rows(&self, rows: Vec<Row>, line: i64) -> Vec<Row> {
        let mut rows = rows;
        let mut unmerged_rows: IndexMap<i64, IndexMap<i64, Cell>> = IndexMap::new();
        let current = rows[line as usize].cells();
        for (column, cell) in current.iter().enumerate() {
            let column = column as i64;
            let cell = cell.clone();
            // PHP validates here that a cell is null, a TableCell, a scalar, or a __toString object.
            // The `Cell` type makes every variant valid, so the InvalidArgumentException is dead.
            if cell.is_table_cell() && cell.rowspan() > 1 {
                let mut nb_lines = cell.rowspan() - 1;
                let cell_str = cell.to_php_string();
                let mut lines = vec![cell.clone()];
                if shirabe_php_shim::strstr(&cell_str, "\n").is_some() {
                    let eol = if shirabe_php_shim::str_contains(&cell_str, "\r\n") {
                        "\r\n"
                    } else {
                        "\n"
                    };
                    let exploded = shirabe_php_shim::explode(
                        eol,
                        &shirabe_php_shim::str_replace(
                            eol,
                            &format!("<fg=default;bg=default>{}</>", eol),
                            &cell_str,
                        ),
                    );
                    lines = exploded.into_iter().map(Cell::Value).collect();
                    nb_lines = if (lines.len() as i64) > nb_lines {
                        shirabe_php_shim::substr_count(&cell_str, eol)
                    } else {
                        nb_lines
                    };

                    let mut r = rows[line as usize].cells();
                    set_cell(
                        &mut r,
                        column,
                        Cell::Cell(TableCell::new2(
                            &lines[0].to_php_string(),
                            Self::table_cell_options_colspan_style(cell.colspan(), cell.style()),
                        )),
                    );
                    rows[line as usize] = Row::Cells(r);
                    lines.remove(0);
                }

                // create a two dimensional array (rowspan x colspan)
                for k in (line + 1)..=(line + nb_lines) {
                    unmerged_rows.entry(k).or_default();
                }
                for unmerged_row_key in unmerged_rows.keys().cloned().collect::<Vec<_>>() {
                    let idx = unmerged_row_key - line;
                    let value = lines
                        .get(idx as usize)
                        .cloned()
                        .unwrap_or(Cell::Value(String::new()));
                    unmerged_rows.get_mut(&unmerged_row_key).unwrap().insert(
                        column,
                        Cell::Cell(TableCell::new2(
                            &value.to_php_string(),
                            Self::table_cell_options_colspan_style(cell.colspan(), cell.style()),
                        )),
                    );
                    if nb_lines == unmerged_row_key - line {
                        break;
                    }
                }
            }
        }

        for (unmerged_row_key, unmerged_row) in unmerged_rows.clone() {
            // we need to know if $unmergedRow will be merged or inserted into $rows
            let fits = (unmerged_row_key as usize) < rows.len()
                && matches!(rows[unmerged_row_key as usize], Row::Cells(_))
                && (self.get_number_of_columns(&rows[unmerged_row_key as usize].cells())
                    + self.get_number_of_columns(
                        &unmerged_rows[&unmerged_row_key]
                            .values()
                            .cloned()
                            .collect::<Vec<_>>(),
                    )
                    <= self.number_of_columns.unwrap());
            if fits {
                let mut target = rows[unmerged_row_key as usize].cells();
                for (cell_key, cell) in unmerged_row {
                    // insert cell into row at cellKey position
                    shirabe_php_shim::array_splice(&mut target, cell_key, Some(0), vec![cell]);
                }
                rows[unmerged_row_key as usize] = Row::Cells(target);
            } else {
                let mut row = self.copy_row(&rows, unmerged_row_key - 1);
                for (column, cell) in &unmerged_row {
                    if cell.is_truthy() {
                        set_cell(&mut row, *column, unmerged_row[column].clone());
                    }
                }
                shirabe_php_shim::array_splice(
                    &mut rows,
                    unmerged_row_key,
                    Some(0),
                    vec![Row::Cells(row)],
                );
            }
        }

        rows
    }

    /// fill cells for a row that contains colspan > 1.
    fn fill_cells(&self, row: Vec<Cell>) -> Vec<Cell> {
        let mut new_row: Vec<Cell> = Vec::new();

        for (column, cell) in row.iter().enumerate() {
            let column = column as i64;
            new_row.push(cell.clone());
            if cell.is_table_cell() && cell.colspan() > 1 {
                for _position in (column + 1)..=(column + cell.colspan() - 1) {
                    // insert empty value at column position
                    new_row.push(Cell::Value(String::new()));
                }
            }
        }

        if new_row.is_empty() { row } else { new_row }
    }

    fn copy_row(&self, rows: &[Row], line: i64) -> Vec<Cell> {
        let mut row = rows[line as usize].cells();
        for cell in &mut row {
            let cell_value = cell.clone();
            *cell = Cell::Value(String::new());
            if cell_value.is_table_cell() {
                *cell = Cell::Cell(TableCell::new2(
                    "",
                    Self::table_cell_options_colspan(cell_value.colspan()),
                ));
            }
        }

        row
    }

    /// Gets number of columns by row.
    fn get_number_of_columns(&self, row: &[Cell]) -> i64 {
        let mut columns = row.len() as i64;
        for column in row {
            columns += if column.is_table_cell() {
                column.colspan() - 1
            } else {
                0
            };
        }

        columns
    }

    /// Gets list of columns for the given row.
    fn get_row_columns(&self, row: &[Cell]) -> Vec<i64> {
        let mut columns: Vec<i64> = (0..self.number_of_columns.unwrap()).collect();
        for (cell_key, cell) in row.iter().enumerate() {
            let cell_key = cell_key as i64;
            if cell.is_table_cell() && cell.colspan() > 1 {
                // exclude grouped columns.
                let excluded: Vec<i64> =
                    ((cell_key + 1)..=(cell_key + cell.colspan() - 1)).collect();
                columns.retain(|c| !excluded.contains(c));
            }
        }

        columns
    }

    /// Calculates columns widths.
    fn calculate_columns_width(&mut self, groups: &TableRows) {
        let mut column = 0;
        while column < self.number_of_columns.unwrap() {
            let mut lengths: Vec<i64> = Vec::new();
            for group in groups {
                for row in group {
                    if row.is_table_separator() {
                        continue;
                    }

                    let mut row_arr = row.cells();
                    for i in 0..row_arr.len() {
                        let cell = row_arr[i].clone();
                        if cell.is_table_cell() {
                            let text_content = self.remove_decoration(&cell.to_php_string());
                            let text_length = Helper::width(&text_content);
                            if text_length > 0 {
                                let content_columns = shirabe_php_shim::mb_str_split(
                                    &text_content,
                                    (text_length as f64 / cell.colspan() as f64).ceil() as i64,
                                );
                                for (position, content) in content_columns.into_iter().enumerate() {
                                    set_cell(
                                        &mut row_arr,
                                        i as i64 + position as i64,
                                        Cell::Value(content),
                                    );
                                }
                            }
                        }
                    }

                    lengths.push(self.get_cell_width(&row_arr, column));
                }
            }

            self.effective_column_widths.insert(
                column,
                *lengths.iter().max().unwrap()
                    + Helper::width(&self.style.get_cell_row_content_format())
                    - 2,
            );
            column += 1;
        }
    }

    fn get_column_separator_width(&self) -> i64 {
        Helper::width(&shirabe_php_shim::sprintf(
            &self.style.get_border_format(),
            &[PhpMixed::from(self.style.get_border_chars()[3].clone())],
        ))
    }

    fn get_cell_width(&self, row: &[Cell], column: i64) -> i64 {
        let mut cell_width = 0;

        if let Some(cell) = row.get(column as usize) {
            cell_width = Helper::width(&self.remove_decoration(&cell.to_php_string()));
        }

        let column_width = *self.column_widths.get(&column).unwrap_or(&0);
        cell_width = cell_width.max(column_width);

        if let Some(max) = self.column_max_widths.get(&column) {
            (*max).min(cell_width)
        } else {
            cell_width
        }
    }

    /// Called after rendering to cleanup cache data.
    fn cleanup(&mut self) {
        self.effective_column_widths = IndexMap::new();
        self.number_of_columns = None;
    }

    fn init_styles() -> IndexMap<String, TableStyle> {
        let mut borderless = TableStyle::default();
        borderless
            .set_horizontal_border_chars("=".to_string(), None)
            .set_vertical_border_chars(" ".to_string(), None)
            .set_default_crossing_char(" ".to_string());

        let mut compact = TableStyle::default();
        compact
            .set_horizontal_border_chars("".to_string(), None)
            .set_vertical_border_chars("".to_string(), None)
            .set_default_crossing_char("".to_string())
            .set_cell_row_content_format("%s ".to_string());

        let mut style_guide = TableStyle::default();
        style_guide
            .set_horizontal_border_chars("-".to_string(), None)
            .set_vertical_border_chars(" ".to_string(), None)
            .set_default_crossing_char(" ".to_string())
            .set_cell_header_format("%s".to_string());

        let mut r#box = TableStyle::default();
        r#box
            .set_horizontal_border_chars("─".to_string(), None)
            .set_vertical_border_chars("│".to_string(), None)
            .set_crossing_chars(
                "┼".to_string(),
                "┌".to_string(),
                "┬".to_string(),
                "┐".to_string(),
                "┤".to_string(),
                "┘".to_string(),
                "┴".to_string(),
                "└".to_string(),
                "├".to_string(),
                None,
                None,
                None,
            );

        let mut box_double = TableStyle::default();
        box_double
            .set_horizontal_border_chars("═".to_string(), Some("─".to_string()))
            .set_vertical_border_chars("║".to_string(), Some("│".to_string()))
            .set_crossing_chars(
                "┼".to_string(),
                "╔".to_string(),
                "╤".to_string(),
                "╗".to_string(),
                "╢".to_string(),
                "╝".to_string(),
                "╧".to_string(),
                "╚".to_string(),
                "╟".to_string(),
                Some("╠".to_string()),
                Some("╪".to_string()),
                Some("╣".to_string()),
            );

        let mut result: IndexMap<String, TableStyle> = IndexMap::new();
        result.insert("default".to_string(), TableStyle::default());
        result.insert("borderless".to_string(), borderless);
        result.insert("compact".to_string(), compact);
        result.insert("symfony-style-guide".to_string(), style_guide);
        result.insert("box".to_string(), r#box);
        result.insert("box-double".to_string(), box_double);

        result
    }

    fn resolve_style(
        &self,
        name: StyleName,
    ) -> anyhow::Result<Result<TableStyle, InvalidArgumentException>> {
        let name = match name {
            StyleName::Style(style) => return Ok(Ok(style)),
            StyleName::Name(name) => name,
        };

        let styles_guard = styles().lock().unwrap();
        if let Some(style) = styles_guard.as_ref().and_then(|s| s.get(&name)) {
            return Ok(Ok(style.clone()));
        }

        Ok(Err(InvalidArgumentException(
            shirabe_php_shim::InvalidArgumentException {
                message: format!("Style \"{}\" is not defined.", name),
                code: 0,
            },
        )))
    }

    fn formatter_is_wrappable(_output: &Rc<RefCell<dyn OutputInterface>>) -> bool {
        // PHP: $this->output->getFormatter() instanceof WrappableOutputFormatterInterface
        // TODO(phase-c/d): instanceof on `dyn OutputFormatterInterface` needs an AsAny supertrait
        // on OutputFormatterInterface to downcast to the concrete wrappable formatter; adding it
        // would touch output_formatter_interface.rs, which is out of scope for this file.
        let _ = std::any::type_name::<dyn WrappableOutputFormatterInterface>();
        todo!()
    }

    /// PHP `Helper::removeDecoration($this->output->getFormatter(), $string)`.
    fn remove_decoration(&self, string: &str) -> String {
        let formatter = self.output.borrow().get_formatter();
        let mut formatter = formatter.borrow_mut();
        Helper::remove_decoration(&mut *formatter, string)
    }

    fn output_is_console_section(output: &Rc<RefCell<dyn OutputInterface>>) -> bool {
        // PHP: $this->output instanceof ConsoleSectionOutput
        let borrowed = output.borrow();
        (*borrowed)
            .as_any()
            .downcast_ref::<ConsoleSectionOutput>()
            .is_some()
    }

    fn table_cell_options_colspan(colspan: i64) -> IndexMap<String, TableCellOption> {
        // PHP: ['colspan' => $colspan]
        let mut options = IndexMap::new();
        options.insert("colspan".to_string(), TableCellOption::Int(colspan));
        options
    }

    fn table_cell_options_colspan_style(
        colspan: i64,
        style: Option<Rc<TableCellStyle>>,
    ) -> IndexMap<String, TableCellOption> {
        // PHP: ['colspan' => $colspan, 'style' => $style]
        let mut options = IndexMap::new();
        options.insert("colspan".to_string(), TableCellOption::Int(colspan));
        options.insert(
            "style".to_string(),
            match style {
                Some(style) => TableCellOption::Style(style),
                None => TableCellOption::Null,
            },
        );
        options
    }
}

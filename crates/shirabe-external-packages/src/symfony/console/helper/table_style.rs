//! ref: composer/vendor/symfony/console/Helper/TableStyle.php

use crate::symfony::console::exception::invalid_argument_exception::InvalidArgumentException;
use crate::symfony::console::exception::logic_exception::LogicException;

/// Defines the styles for a Table.
#[derive(Debug)]
pub struct TableStyle {
    padding_char: String,
    horizontal_outside_border_char: String,
    horizontal_inside_border_char: String,
    vertical_outside_border_char: String,
    vertical_inside_border_char: String,
    crossing_char: String,
    crossing_top_right_char: String,
    crossing_top_mid_char: String,
    crossing_top_left_char: String,
    crossing_mid_right_char: String,
    crossing_bottom_right_char: String,
    crossing_bottom_mid_char: String,
    crossing_bottom_left_char: String,
    crossing_mid_left_char: String,
    crossing_top_left_bottom_char: String,
    crossing_top_mid_bottom_char: String,
    crossing_top_right_bottom_char: String,
    header_title_format: String,
    footer_title_format: String,
    cell_header_format: String,
    cell_row_format: String,
    cell_row_content_format: String,
    border_format: String,
    pad_type: i64,
}

impl Default for TableStyle {
    fn default() -> Self {
        Self {
            padding_char: " ".to_string(),
            horizontal_outside_border_char: "-".to_string(),
            horizontal_inside_border_char: "-".to_string(),
            vertical_outside_border_char: "|".to_string(),
            vertical_inside_border_char: "|".to_string(),
            crossing_char: "+".to_string(),
            crossing_top_right_char: "+".to_string(),
            crossing_top_mid_char: "+".to_string(),
            crossing_top_left_char: "+".to_string(),
            crossing_mid_right_char: "+".to_string(),
            crossing_bottom_right_char: "+".to_string(),
            crossing_bottom_mid_char: "+".to_string(),
            crossing_bottom_left_char: "+".to_string(),
            crossing_mid_left_char: "+".to_string(),
            crossing_top_left_bottom_char: "+".to_string(),
            crossing_top_mid_bottom_char: "+".to_string(),
            crossing_top_right_bottom_char: "+".to_string(),
            header_title_format: "<fg=black;bg=white;options=bold> %s </>".to_string(),
            footer_title_format: "<fg=black;bg=white;options=bold> %s </>".to_string(),
            cell_header_format: "<info>%s</info>".to_string(),
            cell_row_format: "%s".to_string(),
            cell_row_content_format: " %s ".to_string(),
            border_format: "%s".to_string(),
            pad_type: shirabe_php_shim::STR_PAD_RIGHT,
        }
    }
}

impl TableStyle {
    /// Sets padding character, used for cell padding.
    pub fn set_padding_char(
        &mut self,
        padding_char: String,
    ) -> anyhow::Result<Result<&mut Self, LogicException>> {
        if padding_char.is_empty() {
            return Ok(Err(LogicException(shirabe_php_shim::LogicException {
                message: "The padding char must not be empty.".to_string(),
                code: 0,
            })));
        }

        self.padding_char = padding_char;

        Ok(Ok(self))
    }

    /// Gets padding character, used for cell padding.
    pub fn get_padding_char(&self) -> String {
        self.padding_char.clone()
    }

    /// Sets horizontal border characters.
    pub fn set_horizontal_border_chars(
        &mut self,
        outside: String,
        inside: Option<String>,
    ) -> &mut Self {
        self.horizontal_outside_border_char = outside.clone();
        self.horizontal_inside_border_char = inside.unwrap_or(outside);

        self
    }

    /// Sets vertical border characters.
    pub fn set_vertical_border_chars(
        &mut self,
        outside: String,
        inside: Option<String>,
    ) -> &mut Self {
        self.vertical_outside_border_char = outside.clone();
        self.vertical_inside_border_char = inside.unwrap_or(outside);

        self
    }

    /// Gets border characters.
    pub fn get_border_chars(&self) -> Vec<String> {
        vec![
            self.horizontal_outside_border_char.clone(),
            self.vertical_outside_border_char.clone(),
            self.horizontal_inside_border_char.clone(),
            self.vertical_inside_border_char.clone(),
        ]
    }

    /// Sets crossing characters.
    #[allow(clippy::too_many_arguments)]
    pub fn set_crossing_chars(
        &mut self,
        cross: String,
        top_left: String,
        top_mid: String,
        top_right: String,
        mid_right: String,
        bottom_right: String,
        bottom_mid: String,
        bottom_left: String,
        mid_left: String,
        top_left_bottom: Option<String>,
        top_mid_bottom: Option<String>,
        top_right_bottom: Option<String>,
    ) -> &mut Self {
        self.crossing_char = cross.clone();
        self.crossing_top_left_char = top_left;
        self.crossing_top_mid_char = top_mid;
        self.crossing_top_right_char = top_right;
        self.crossing_mid_right_char = mid_right.clone();
        self.crossing_bottom_right_char = bottom_right;
        self.crossing_bottom_mid_char = bottom_mid;
        self.crossing_bottom_left_char = bottom_left;
        self.crossing_mid_left_char = mid_left.clone();
        self.crossing_top_left_bottom_char = top_left_bottom.unwrap_or(mid_left);
        self.crossing_top_mid_bottom_char = top_mid_bottom.unwrap_or(cross);
        self.crossing_top_right_bottom_char = top_right_bottom.unwrap_or(mid_right);

        self
    }

    /// Sets default crossing character used for each cross.
    pub fn set_default_crossing_char(&mut self, char: String) -> &mut Self {
        self.set_crossing_chars(
            char.clone(),
            char.clone(),
            char.clone(),
            char.clone(),
            char.clone(),
            char.clone(),
            char.clone(),
            char.clone(),
            char,
            None,
            None,
            None,
        )
    }

    /// Gets crossing character.
    pub fn get_crossing_char(&self) -> String {
        self.crossing_char.clone()
    }

    /// Gets crossing characters.
    pub fn get_crossing_chars(&self) -> Vec<String> {
        vec![
            self.crossing_char.clone(),
            self.crossing_top_left_char.clone(),
            self.crossing_top_mid_char.clone(),
            self.crossing_top_right_char.clone(),
            self.crossing_mid_right_char.clone(),
            self.crossing_bottom_right_char.clone(),
            self.crossing_bottom_mid_char.clone(),
            self.crossing_bottom_left_char.clone(),
            self.crossing_mid_left_char.clone(),
            self.crossing_top_left_bottom_char.clone(),
            self.crossing_top_mid_bottom_char.clone(),
            self.crossing_top_right_bottom_char.clone(),
        ]
    }

    /// Sets header cell format.
    pub fn set_cell_header_format(&mut self, cell_header_format: String) -> &mut Self {
        self.cell_header_format = cell_header_format;

        self
    }

    /// Gets header cell format.
    pub fn get_cell_header_format(&self) -> String {
        self.cell_header_format.clone()
    }

    /// Sets row cell format.
    pub fn set_cell_row_format(&mut self, cell_row_format: String) -> &mut Self {
        self.cell_row_format = cell_row_format;

        self
    }

    /// Gets row cell format.
    pub fn get_cell_row_format(&self) -> String {
        self.cell_row_format.clone()
    }

    /// Sets row cell content format.
    pub fn set_cell_row_content_format(&mut self, cell_row_content_format: String) -> &mut Self {
        self.cell_row_content_format = cell_row_content_format;

        self
    }

    /// Gets row cell content format.
    pub fn get_cell_row_content_format(&self) -> String {
        self.cell_row_content_format.clone()
    }

    /// Sets table border format.
    pub fn set_border_format(&mut self, border_format: String) -> &mut Self {
        self.border_format = border_format;

        self
    }

    /// Gets table border format.
    pub fn get_border_format(&self) -> String {
        self.border_format.clone()
    }

    /// Sets cell padding type.
    pub fn set_pad_type(
        &mut self,
        pad_type: i64,
    ) -> anyhow::Result<Result<&mut Self, InvalidArgumentException>> {
        if ![
            shirabe_php_shim::STR_PAD_LEFT,
            shirabe_php_shim::STR_PAD_RIGHT,
            shirabe_php_shim::STR_PAD_BOTH,
        ]
        .contains(&pad_type)
        {
            return Ok(Err(InvalidArgumentException(
                shirabe_php_shim::InvalidArgumentException {
                    message: "Invalid padding type. Expected one of (STR_PAD_LEFT, STR_PAD_RIGHT, STR_PAD_BOTH)."
                        .to_string(),
                    code: 0,
                },
            )));
        }

        self.pad_type = pad_type;

        Ok(Ok(self))
    }

    /// Gets cell padding type.
    pub fn get_pad_type(&self) -> i64 {
        self.pad_type
    }

    pub fn get_header_title_format(&self) -> String {
        self.header_title_format.clone()
    }

    pub fn set_header_title_format(&mut self, format: String) -> &mut Self {
        self.header_title_format = format;

        self
    }

    pub fn get_footer_title_format(&self) -> String {
        self.footer_title_format.clone()
    }

    pub fn set_footer_title_format(&mut self, format: String) -> &mut Self {
        self.footer_title_format = format;

        self
    }
}

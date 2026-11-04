//! ref: composer/vendor/symfony/console/Helper/TableStyle.php sha1:41923866396e55c48a949bbb29cac419b6465252

use bstr::BString;
use shirabe_php_shim::STR_PAD_RIGHT;

/// Defines the styles for a Table.
#[derive(Debug, Clone)]
pub struct TableStyle {
    padding_char: BString,
    horizontal_outside_border_char: BString,
    horizontal_inside_border_char: BString,
    vertical_outside_border_char: BString,
    vertical_inside_border_char: BString,
    crossing_char: BString,
    crossing_top_right_char: BString,
    crossing_top_mid_char: BString,
    crossing_top_left_char: BString,
    crossing_mid_right_char: BString,
    crossing_bottom_right_char: BString,
    crossing_bottom_mid_char: BString,
    crossing_bottom_left_char: BString,
    crossing_mid_left_char: BString,
    crossing_top_left_bottom_char: BString,
    crossing_top_mid_bottom_char: BString,
    crossing_top_right_bottom_char: BString,
    cell_header_format: BString,
    cell_row_format: BString,
    cell_row_content_format: BString,
    border_format: BString,
    pad_type: i64,
}

impl Default for TableStyle {
    fn default() -> Self {
        Self {
            padding_char: " ".into(),
            horizontal_outside_border_char: "-".into(),
            horizontal_inside_border_char: "-".into(),
            vertical_outside_border_char: "|".into(),
            vertical_inside_border_char: "|".into(),
            crossing_char: "+".into(),
            crossing_top_right_char: "+".into(),
            crossing_top_mid_char: "+".into(),
            crossing_top_left_char: "+".into(),
            crossing_mid_right_char: "+".into(),
            crossing_bottom_right_char: "+".into(),
            crossing_bottom_mid_char: "+".into(),
            crossing_bottom_left_char: "+".into(),
            crossing_mid_left_char: "+".into(),
            crossing_top_left_bottom_char: "+".into(),
            crossing_top_mid_bottom_char: "+".into(),
            crossing_top_right_bottom_char: "+".into(),
            cell_header_format: "<info>%s</info>".into(),
            cell_row_format: "%s".into(),
            cell_row_content_format: " %s ".into(),
            border_format: "%s".into(),
            pad_type: STR_PAD_RIGHT,
        }
    }
}

impl TableStyle {
    /// Gets padding character, used for cell padding.
    pub fn get_padding_char(&self) -> BString {
        self.padding_char.clone()
    }

    /// Sets horizontal border characters.
    pub fn set_horizontal_border_chars(
        &mut self,
        outside: BString,
        inside: Option<BString>,
    ) -> &mut Self {
        self.horizontal_outside_border_char = outside.clone();
        self.horizontal_inside_border_char = inside.unwrap_or(outside);

        self
    }

    /// Sets vertical border characters.
    pub fn set_vertical_border_chars(
        &mut self,
        outside: BString,
        inside: Option<BString>,
    ) -> &mut Self {
        self.vertical_outside_border_char = outside.clone();
        self.vertical_inside_border_char = inside.unwrap_or(outside);

        self
    }

    /// Gets border characters.
    pub fn get_border_chars(&self) -> Vec<BString> {
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
        cross: BString,
        top_left: BString,
        top_mid: BString,
        top_right: BString,
        mid_right: BString,
        bottom_right: BString,
        bottom_mid: BString,
        bottom_left: BString,
        mid_left: BString,
        top_left_bottom: Option<BString>,
        top_mid_bottom: Option<BString>,
        top_right_bottom: Option<BString>,
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
    pub fn set_default_crossing_char(&mut self, char: BString) -> &mut Self {
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
    pub fn get_crossing_char(&self) -> BString {
        self.crossing_char.clone()
    }

    /// Gets crossing characters.
    pub fn get_crossing_chars(&self) -> Vec<BString> {
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
    pub fn set_cell_header_format(&mut self, cell_header_format: BString) -> &mut Self {
        self.cell_header_format = cell_header_format;

        self
    }

    /// Gets header cell format.
    pub fn get_cell_header_format(&self) -> BString {
        self.cell_header_format.clone()
    }

    /// Gets row cell format.
    pub fn get_cell_row_format(&self) -> BString {
        self.cell_row_format.clone()
    }

    /// Sets row cell content format.
    pub fn set_cell_row_content_format(&mut self, cell_row_content_format: BString) -> &mut Self {
        self.cell_row_content_format = cell_row_content_format;

        self
    }

    /// Gets row cell content format.
    pub fn get_cell_row_content_format(&self) -> BString {
        self.cell_row_content_format.clone()
    }

    /// Gets table border format.
    pub fn get_border_format(&self) -> BString {
        self.border_format.clone()
    }

    /// Gets cell padding type.
    pub fn get_pad_type(&self) -> i64 {
        self.pad_type
    }
}

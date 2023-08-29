// A lot of the below is based upon https://github.com/AlexanderThaller/format_serde_error/tree/main
// which is licensed under the MIT license. Thank you!

use std::fmt;

use colored::Colorize;

/// Separator used between the line numbering and the lines.
const SEPARATOR: &str = " | ";

/// Ellipse used to indicated if a long line has been contextualized.
const ELLIPSE: &str = "...";

/// Struct for formatting the error together with the source file to give a
/// nicer output.
#[derive(Debug)]
pub struct KclError {
    input: String,
    message: String,
    line: Option<usize>,
    column: Option<usize>,
    contextualize: bool,
    context_lines: usize,
    context_characters: usize,
}

/// The error types that we can pretty format.
#[derive(Debug)]
pub enum ErrorTypes {
    /// Contains [`kcl_lib::errors::KclError`].
    Kcl(kcl_lib::errors::KclError),
}

impl std::error::Error for KclError {}

impl fmt::Display for KclError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.format(f)
    }
}

impl From<kcl_lib::errors::KclError> for ErrorTypes {
    fn from(err: kcl_lib::errors::KclError) -> Self {
        Self::Kcl(err)
    }
}

impl KclError {
    /// Create a new [`KclError`] from compatible errors. See
    /// [`ErrorTypes`] for more information.
    pub fn new(input: String, err: impl Into<ErrorTypes>) -> KclError {
        let error = err.into();

        let (message, line, column) = match error {
            ErrorTypes::Kcl(err) => err.get_message_line_column(&input),
        };

        Self {
            input,
            message,
            line,
            column,
            // If the output should be contextualized or not.
            contextualize: true,
            // Amount of lines to show before and after the line containing the error.
            context_lines: 3,
            // Amount of characters to show before and after the column containing the
            // error.
            context_characters: 30,
        }
    }

    /// Set if the output should be contextualized or not.
    /// By default contextualization is set to [`CONTEXTUALIZE_DEFAULT`].
    pub fn set_contextualize(&mut self, should_contextualize: bool) -> &mut Self {
        self.contextualize = should_contextualize;
        self
    }

    /// Get if the output should be contextualized or not.
    /// By default contextualization is set to [`CONTEXTUALIZE_DEFAULT`].
    #[must_use]
    pub fn get_contextualize(&self) -> bool {
        self.contextualize
    }

    /// Set the amount of lines that should be shown before and after the error.
    /// By default the amount of context is set to [`CONTEXT_LINES_DEFAULT`].
    pub fn set_context_lines(&mut self, amount_of_context: usize) -> &mut Self {
        self.context_lines = amount_of_context;
        self
    }

    /// Get the amount of lines that should be shown before and after the error.
    #[must_use]
    pub fn get_context_lines(&self) -> usize {
        self.context_lines
    }

    /// Set the amount of characters that should be shown before and after the
    /// error. By default the amount of context is set to
    /// [`CONTEXT_CHARACTERS_DEFAULT`].
    pub fn set_context_characters(&mut self, amount_of_context: usize) -> &mut Self {
        self.context_characters = amount_of_context;
        self
    }

    /// Get the amount of characters that should be shown before and after the
    /// error. Default value is [`CONTEXT_CHARACTERS_DEFAULT`].
    #[must_use]
    pub fn get_context_characters(&self) -> usize {
        self.context_characters
    }

    fn format(&self, f: &mut fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        // If line and column are not set we assume that we can't make a nice output
        // so we will just print the original message in red and bold
        if self.line.is_none() && self.column.is_none() {
            return writeln!(f, "{}", self.message.red().bold());
        }

        let error_line = self.line.unwrap_or_default();
        let error_column = self.column.unwrap_or_default();

        // Amount of lines to show before and after the error line
        let context_lines = self.context_lines;

        // Skip until we are amount of context lines before the error line (context)
        // plus the line with the error ( + 1)
        // Saturating sub if the error is in the first few line we can't take more
        // context
        let skip = usize::saturating_sub(error_line, context_lines + 1);

        // Take lines before and after (context * 2) plus the line with the error ( + 1)
        let take = context_lines * 2 + 1;

        // Minimize the input to only what we need so we can reuse it without
        // having to iterate over the whole input again.
        // Also replace tabs with two spaces
        let minimized_input = self
            .input
            .lines()
            .skip(skip)
            .take(take)
            .map(|line| line.replace('\t', " "))
            .collect::<Vec<_>>();

        // If the minimized_input is empty we can assume that the input was empty as
        // well. In that case we can't make a nice output so we will just print
        // the original message in red and bold
        if minimized_input.is_empty() {
            return writeln!(f, "{}", self.message.red().bold());
        }

        // To reduce the amount of space text takes we want to remove unnecessary
        // whitespace in front of the text.
        // Find the line with the least amount of whitespace in front and use
        // that to remove the whitespace later.
        // We basically want to find the least indented line.
        // We cant just use trim as that would remove all whitespace and remove all
        // indentation.
        let whitespace_count = minimized_input
            .iter()
            .map(|line| line.chars().take_while(|s| s.is_whitespace()).count())
            .min()
            .unwrap_or_default();

        let separator = SEPARATOR.blue().bold();

        // When we don't print the line_position we want to fill up the space not used
        // by the line_position with whitespace instead
        let fill_line_position = format!("{: >fill$}", "", fill = error_line.to_string().len());

        // Want to avoid printing when we are not at the beginning of the line. For
        // example anyhow will write 'Error:' in front of the output before
        // printing the buffer
        writeln!(f)?;

        self.input
            .lines()
            .enumerate()
            .skip(skip)
            .take(take)
            .map(|(index, text)| {
                // Make the index start at 1 makes it nicer to work with
                // Also remove unnecessary whitespace in front of text
                (
                    index + 1,
                    text.chars()
                        .skip(whitespace_count)
                        .collect::<String>()
                        .replace('\t', " "),
                )
            })
            .try_for_each(|(line_position, text)| {
                self.format_line(
                    f,
                    line_position,
                    error_line,
                    error_column,
                    text,
                    whitespace_count,
                    &separator,
                    &fill_line_position,
                )
            })?;

        Ok(())
    }

    // TODO: Maybe make another internal struct for formatting instead of having
    // this list of args.
    #[allow(clippy::too_many_arguments)]
    fn format_line(
        &self,
        f: &mut fmt::Formatter<'_>,
        line_position: usize,
        error_line: usize,
        error_column: usize,
        text: String,
        whitespace_count: usize,
        separator: &colored::ColoredString,
        fill_line_position: &str,
    ) -> Result<(), std::fmt::Error> {
        if line_position == error_line {
            let long_line_threshold = self.context_characters * 2 + 1;
            let long_line_threshold = long_line_threshold < text.len();

            let (context_line, new_error_column, context_before, context_after) =
                if self.contextualize && long_line_threshold {
                    let context_characters = self.context_characters;
                    Self::context_long_line(&text, error_column, context_characters)
                } else {
                    (text, error_column, false, false)
                };

            Self::format_error_line(
                f,
                &context_line,
                line_position,
                separator,
                context_before,
                context_after,
            )?;

            self.format_error_information(
                f,
                whitespace_count,
                separator,
                fill_line_position,
                new_error_column,
                context_before,
            )
        } else if self.contextualize {
            Self::format_context_line(f, &text, separator, fill_line_position)
        } else {
            Ok(())
        }
    }

    fn format_error_line(
        f: &mut fmt::Formatter<'_>,
        text: &str,
        line_position: usize,
        separator: &colored::ColoredString,
        context_before: bool,
        context_after: bool,
    ) -> Result<(), std::fmt::Error> {
        let line_pos = line_position.to_string().blue().bold();

        write!(f, " {}{}", line_pos, separator)?;

        if context_before {
            write!(f, "{}", (ELLIPSE.blue().bold()))?;
        }

        write!(f, "{}", text)?;

        if context_after {
            write!(f, "{}", (ELLIPSE.blue().bold()))?;
        }

        writeln!(f)
    }

    fn format_error_information(
        &self,
        f: &mut fmt::Formatter<'_>,
        whitespace_count: usize,
        separator: &colored::ColoredString,
        fill_line_position: &str,
        error_column: usize,
        context_before: bool,
    ) -> Result<(), std::fmt::Error> {
        let ellipse_space = if context_before { ELLIPSE.len() } else { 0 };

        // Print whitespace until we reach the column value of the message. We also
        // have to add the amount of whitespace in front of the other lines.
        // If context_before is true we also need to add the space used by the ellipse
        let fill_column_position = format!(
            "{: >column$}^ {}",
            "",
            self.message,
            column = error_column - whitespace_count + ellipse_space
        );

        let fill_column_position = fill_column_position.red().bold();

        writeln!(f, " {}{}{}", fill_line_position, separator, fill_column_position,)
    }

    fn format_context_line(
        f: &mut fmt::Formatter<'_>,
        text: &str,
        separator: &colored::ColoredString,

        fill_line_position: &str,
    ) -> Result<(), std::fmt::Error> {
        writeln!(f, " {}{}{}", fill_line_position, separator, text.yellow())
    }

    fn context_long_line(text: &str, error_column: usize, context_chars: usize) -> (String, usize, bool, bool) {
        use unicode_segmentation::UnicodeSegmentation;

        // As we could deal with unicode we can have characters that are multiple code
        // points. In that case we do not want to iterate over each code point
        // (i.e. using text.chars()) we need to use graphemes instead.
        let input = text.graphemes(true).collect::<Vec<_>>();

        // Skip until we are amount of context chars before the error column (context)
        // plus the column with the error ( + 1) Saturating sub if the error is
        // in the first few chars we can't take more context
        let skip = usize::saturating_sub(error_column, context_chars + 1);

        // Take chars before and after (context_chars * 2) plus the column with the
        // error ( + 1)
        let take = context_chars * 2 + 1;

        // If we skipped any characters that means we are contextualizing before the
        // error. That means that we need to print ... at the beginning of the error
        // line later on in the code.
        let context_before = skip != 0;

        // If the line is bigger than skipping and taking combined that means that we
        // not getting the remaining text of the line after the error. That
        // means that we need to print ... at the end of the error line later on
        // in the code.
        let context_after = skip + take < input.len();

        let minimized_input = input.into_iter().skip(skip).take(take).collect();

        // Error column has moved to the right as we skipped some characters so we need
        // to update it. Saturating sub as the error could be at the beginning
        // of the line.
        let new_error_column = usize::saturating_sub(error_column, skip);

        (minimized_input, new_error_column, context_before, context_after)
    }
}

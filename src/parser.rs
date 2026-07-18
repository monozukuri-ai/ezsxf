//! Part 21/SFC syntax parser and resolved SFC document construction.

use std::path::Path;

use encoding_rs::SHIFT_JIS;

use crate::model::*;

#[derive(Clone, Copy)]
struct Snapshot {
    index: usize,
    line: usize,
    column: usize,
}

pub(crate) struct Parser<'a> {
    input: &'a str,
    chars: Vec<char>,
    index: usize,
    line: usize,
    column: usize,
    pub(crate) format: FileFormat,
    strict: bool,
    warnings: Vec<ParseWarning>,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str, format: FileFormat, strict: bool) -> Self {
        Self {
            input,
            chars: input.chars().collect(),
            index: 0,
            line: 1,
            column: 1,
            format,
            strict,
            warnings: Vec::new(),
        }
    }

    fn parse(mut self) -> Result<ParseOutput, ParseError> {
        self.skip_ws();
        self.consume_literal_ci("ISO-10303-21")?;
        self.skip_ws();
        self.consume_char(';')?;
        let header = self.parse_header_section()?;
        let entities = match self.format {
            FileFormat::P21 => self.parse_data_section_p21()?,
            FileFormat::Sfc => self.parse_data_section_sfc()?,
        };
        self.skip_ws();
        self.consume_literal_ci("END-ISO-10303-21")?;
        self.skip_ws();
        self.consume_char(';')?;
        self.skip_ws();
        if !self.is_eof() {
            self.issue_or_error(
                "trailing-data",
                "Unexpected trailing data after END-ISO-10303-21;".to_string(),
            )?;
        }

        let mut document = ParsedDocument {
            format: self.format,
            header,
            entities,
            typed_features: Vec::new(),
            sfc_model: None,
        };
        if self.format == FileFormat::Sfc {
            document.typed_features = self.extract_typed_features(&document.entities)?;
            document.sfc_model = Some(self.build_sfc_model(&document)?);
        }
        self.validate_document(&document)?;

        Ok(ParseOutput {
            document,
            warnings: self.warnings,
        })
    }

    fn parse_header_section(&mut self) -> Result<HeaderSection, ParseError> {
        self.skip_ws();
        self.consume_keyword_ci("HEADER")?;
        self.skip_ws();
        self.consume_char(';')?;

        let mut entities = Vec::new();
        loop {
            self.skip_ws();
            if self.try_consume_keyword_ci("ENDSEC") {
                self.skip_ws();
                self.consume_char(';')?;
                break;
            }
            let record = self.parse_simple_record()?;
            self.skip_ws();
            self.consume_char(';')?;
            entities.push(record);
        }

        Ok(HeaderSection { entities })
    }

    fn parse_data_section_p21(&mut self) -> Result<Vec<EntityInstance>, ParseError> {
        self.skip_ws();
        self.consume_keyword_ci("DATA")?;
        self.skip_ws();
        self.consume_char(';')?;

        let mut entities = Vec::new();
        loop {
            self.skip_ws();
            if self.try_consume_keyword_ci("ENDSEC") {
                self.skip_ws();
                self.consume_char(';')?;
                break;
            }
            entities.push(self.parse_entity_instance(true, None)?);
        }
        Ok(entities)
    }

    fn parse_data_section_sfc(&mut self) -> Result<Vec<EntityInstance>, ParseError> {
        self.skip_ws();
        self.consume_keyword_ci("DATA")?;
        self.skip_ws();
        self.consume_char(';')?;

        let mut entities = Vec::new();
        loop {
            self.skip_ws();
            if self.try_consume_keyword_ci("ENDSEC") {
                self.skip_ws();
                self.consume_char(';')?;
                break;
            }
            let tag = match self.parse_sfc_prefix() {
                Ok(tag) => tag,
                Err(error) if !self.strict => {
                    self.push_warning(
                        "sfc-block-prefix",
                        format!("Invalid SFC block prefix skipped: {}", error.message),
                    );
                    if self.seek_next_sfc_prefix_or_endsec() {
                        continue;
                    }
                    break;
                }
                Err(error) => return Err(error),
            };
            self.skip_ws();
            match self.parse_entity_instance(false, Some(tag)) {
                Ok(entity) => {
                    if matches!(&entity.body, EntityBody::Complex(_)) {
                        self.issue_or_error(
                            "sfc-complex-entity",
                            format!(
                                "SFC feature block #{} must contain one simple feature record",
                                entity.id
                            ),
                        )?;
                    }
                    entities.push(entity);
                }
                Err(error) if !self.strict => {
                    self.push_warning(
                        "sfc-feature-block-skipped",
                        format!("Failed to parse feature block; skipped: {}", error.message),
                    );
                    if !self.recover_to_sfc_suffix(tag) {
                        break;
                    }
                    continue;
                }
                Err(error) => return Err(error),
            }
            self.skip_ws();
            match self.parse_sfc_suffix(tag) {
                Ok(()) => {}
                Err(error) if !self.strict => {
                    self.push_warning(
                        "sfc-block-suffix",
                        format!("Invalid SFC block suffix; skipped: {}", error.message),
                    );
                    if !self.recover_to_sfc_suffix(tag) {
                        break;
                    }
                }
                Err(error) => return Err(error),
            }
        }
        Ok(entities)
    }

    fn parse_sfc_prefix(&mut self) -> Result<SfcVersionTag, ParseError> {
        self.skip_ws();
        self.consume_char('/')?;
        self.consume_char('*')?;
        if self.try_consume_literal_ci("SXF3.1") {
            Ok(SfcVersionTag::V31)
        } else if self.try_consume_literal_ci("SXF3") {
            Ok(SfcVersionTag::V3)
        } else if self.try_consume_literal_ci("SXF") {
            Ok(SfcVersionTag::V2)
        } else {
            Err(self.error_here("Invalid SFC block prefix. Expected /*SXF, /*SXF3 or /*SXF3.1"))
        }
    }

    fn parse_sfc_suffix(&mut self, tag: SfcVersionTag) -> Result<(), ParseError> {
        match tag {
            SfcVersionTag::V2 => self.consume_literal_ci("SXF*/"),
            SfcVersionTag::V3 => self.consume_literal_ci("SXF3*/"),
            SfcVersionTag::V31 => self.consume_literal_ci("SXF3.1*/"),
        }
    }

    fn recover_to_sfc_suffix(&mut self, tag: SfcVersionTag) -> bool {
        let suffix = match tag {
            SfcVersionTag::V2 => "SXF*/",
            SfcVersionTag::V3 => "SXF3*/",
            SfcVersionTag::V31 => "SXF3.1*/",
        };
        self.seek_literal_ci(suffix)
    }

    fn seek_next_sfc_prefix_or_endsec(&mut self) -> bool {
        loop {
            if self.is_eof() {
                return false;
            }
            let snapshot = self.snapshot();
            if self.try_consume_keyword_ci("ENDSEC") {
                self.restore(snapshot);
                return true;
            }
            self.restore(snapshot);
            if self.try_consume_literal_ci("/*SXF")
                || self.try_consume_literal_ci("/*SXF3")
                || self.try_consume_literal_ci("/*SXF3.1")
            {
                self.restore(snapshot);
                return true;
            }
            self.advance_char();
        }
    }

    fn parse_entity_instance(
        &mut self,
        semicolon_required: bool,
        sfc_version: Option<SfcVersionTag>,
    ) -> Result<EntityInstance, ParseError> {
        self.skip_ws();
        let id = self.parse_reference_id()?;
        self.skip_ws();
        self.consume_char('=')?;
        self.skip_ws();

        let body = if self.peek_char() == Some('(') {
            EntityBody::Complex(self.parse_complex_record()?)
        } else {
            EntityBody::Simple(self.parse_simple_record()?)
        };

        self.skip_ws();
        if semicolon_required {
            self.consume_char(';')?;
        } else {
            let _ = self.try_consume_char(';');
        }

        Ok(EntityInstance {
            id,
            body,
            sfc_version,
        })
    }

    fn parse_complex_record(&mut self) -> Result<Vec<Record>, ParseError> {
        self.consume_char('(')?;
        self.skip_ws();
        let mut records = Vec::new();
        while self.peek_char() != Some(')') {
            records.push(self.parse_simple_record()?);
            self.skip_ws();
            let _ = self.try_consume_char(',');
            self.skip_ws();
        }
        self.consume_char(')')?;

        if records.is_empty() {
            Err(self.error_here("Complex record must include at least one simple record"))
        } else {
            Ok(records)
        }
    }

    fn parse_simple_record(&mut self) -> Result<Record, ParseError> {
        self.skip_ws();
        let keyword = self.parse_keyword()?;
        self.skip_ws();
        self.consume_char('(')?;
        let parameters = self.parse_parameter_list_until(')')?;
        self.consume_char(')')?;

        Ok(Record {
            keyword,
            parameters,
        })
    }

    fn parse_parameter_list_until(&mut self, terminator: char) -> Result<Vec<Value>, ParseError> {
        self.skip_ws();
        let mut parameters = Vec::new();
        if self.peek_char() == Some(terminator) {
            return Ok(parameters);
        }

        loop {
            parameters.push(self.parse_value()?);
            self.skip_ws();
            if self.try_consume_char(',') {
                self.skip_ws();
                continue;
            }
            break;
        }
        Ok(parameters)
    }

    fn parse_value(&mut self) -> Result<Value, ParseError> {
        self.skip_ws();
        let ch = self
            .peek_char()
            .ok_or_else(|| self.error_here("Unexpected end of input while parsing value"))?;

        match ch {
            '$' => {
                self.advance_char();
                Ok(Value::Unset)
            }
            '*' => {
                self.advance_char();
                Ok(Value::Omitted)
            }
            '#' => Ok(Value::Reference(self.parse_reference_id()?)),
            '\\' if self.peek_next_char() == Some('\'') => {
                Ok(Value::String(self.parse_backslash_quoted_string()?))
            }
            '\'' => Ok(Value::String(self.parse_quoted_string()?)),
            '"' => Ok(Value::Binary(self.parse_binary_literal()?)),
            '(' => {
                self.consume_char('(')?;
                let values = self.parse_parameter_list_until(')')?;
                self.consume_char(')')?;
                Ok(Value::List(values))
            }
            '.' => Ok(Value::Enum(self.parse_enum_value()?)),
            '+' | '-' | '0'..='9' => self.parse_number_value(),
            _ if is_keyword_start(ch) => {
                let keyword = self.parse_keyword()?;
                self.skip_ws();
                if self.try_consume_char('(') {
                    let parameters = self.parse_parameter_list_until(')')?;
                    self.consume_char(')')?;
                    Ok(Value::Typed {
                        keyword,
                        parameters,
                    })
                } else {
                    Err(self.error_here("Bare keyword is not allowed in parameter values"))
                }
            }
            _ => Err(self.error_here("Unsupported value token")),
        }
    }

    fn parse_number_value(&mut self) -> Result<Value, ParseError> {
        let start = self.snapshot();
        let mut text = String::new();
        if let Some(ch @ ('+' | '-')) = self.peek_char() {
            text.push(ch);
            self.advance_char();
        }

        let mut integer_digits = 0usize;
        while let Some(ch) = self.peek_char() {
            if ch.is_ascii_digit() {
                text.push(ch);
                self.advance_char();
                integer_digits += 1;
            } else {
                break;
            }
        }

        if integer_digits == 0 {
            return Err(self.error_at(start, "Invalid numeric literal"));
        }

        let mut is_real = false;
        if self.peek_char() == Some('.') {
            is_real = true;
            text.push('.');
            self.advance_char();
            while let Some(ch) = self.peek_char() {
                if ch.is_ascii_digit() {
                    text.push(ch);
                    self.advance_char();
                } else {
                    break;
                }
            }
        }

        let mut uses_exponent = false;
        if let Some(ch @ ('E' | 'e')) = self.peek_char() {
            uses_exponent = true;
            is_real = true;
            text.push(ch);
            self.advance_char();

            if let Some(sign @ ('+' | '-')) = self.peek_char() {
                text.push(sign);
                self.advance_char();
            }

            let mut exponent_digits = 0usize;
            while let Some(ch) = self.peek_char() {
                if ch.is_ascii_digit() {
                    text.push(ch);
                    self.advance_char();
                    exponent_digits += 1;
                } else {
                    break;
                }
            }

            if exponent_digits == 0 {
                return Err(self.error_at(start, "Invalid exponent in numeric literal"));
            }
        }

        if uses_exponent && self.format == FileFormat::Sfc {
            self.issue_or_error(
                "sfc-real-format",
                "Exponent notation is discouraged for SFC real values".to_string(),
            )?;
        }

        if is_real {
            text.parse::<f64>()
                .map(Value::Real)
                .map_err(|_| self.error_at(start, "Failed to parse floating-point value"))
        } else {
            text.parse::<i64>()
                .map(Value::Integer)
                .map_err(|_| self.error_at(start, "Failed to parse integer value"))
        }
    }

    fn parse_reference_id(&mut self) -> Result<i64, ParseError> {
        let start = self.snapshot();
        self.consume_char('#')?;
        let mut digits = String::new();
        while let Some(ch) = self.peek_char() {
            if ch.is_ascii_digit() {
                digits.push(ch);
                self.advance_char();
            } else {
                break;
            }
        }
        if digits.is_empty() {
            return Err(self.error_at(start, "Reference must contain at least one digit"));
        }
        digits
            .parse::<i64>()
            .map_err(|_| self.error_at(start, "Reference id is out of range"))
    }

    fn parse_quoted_string(&mut self) -> Result<String, ParseError> {
        self.consume_char('\'')?;
        let mut out = String::new();
        loop {
            match self.peek_char() {
                Some('\'') => {
                    self.advance_char();
                    if self.peek_char() == Some('\'') {
                        self.advance_char();
                        out.push('\'');
                    } else {
                        break;
                    }
                }
                Some('\\') => {
                    self.advance_char();
                    if self.peek_char() == Some('\\') {
                        self.advance_char();
                        out.push('\\');
                    } else {
                        out.push('\\');
                    }
                }
                Some(ch) => {
                    self.advance_char();
                    out.push(ch);
                }
                None => {
                    return Err(self.error_here("Unterminated quoted string literal"));
                }
            }
        }
        self.validate_sfc_string(out)
    }

    fn parse_backslash_quoted_string(&mut self) -> Result<String, ParseError> {
        self.consume_char('\\')?;
        self.consume_char('\'')?;
        let mut out = String::new();
        loop {
            match self.peek_char() {
                Some('\\') if self.peek_next_char() == Some('\'') => {
                    self.advance_char();
                    self.advance_char();
                    break;
                }
                Some('\\') => {
                    self.advance_char();
                    if self.peek_char() == Some('\\') {
                        self.advance_char();
                        out.push('\\');
                    } else {
                        out.push('\\');
                    }
                }
                Some('\'') => {
                    if matches!(self.peek_following_non_ws_char(1), Some(',') | Some(')')) {
                        self.advance_char();
                        break;
                    }
                    self.advance_char();
                    out.push('\'');
                }
                Some(ch) => {
                    self.advance_char();
                    out.push(ch);
                }
                None => {
                    return Err(self.error_here("Unterminated quoted string literal"));
                }
            }
        }
        self.validate_sfc_string(out)
    }

    fn validate_sfc_string(&mut self, value: String) -> Result<String, ParseError> {
        if self.format != FileFormat::Sfc {
            return Ok(value);
        }
        if value.contains('\0') {
            self.issue_or_error(
                "sfc-string-nul",
                "SFC strings must not contain NUL characters".to_string(),
            )?;
        }
        let (_, _, had_errors) = SHIFT_JIS.encode(&value);
        if had_errors {
            self.issue_or_error(
                "sfc-string-encoding",
                "SFC string contains characters that cannot be represented in Shift-JIS"
                    .to_string(),
            )?;
        }
        Ok(value)
    }

    fn parse_binary_literal(&mut self) -> Result<String, ParseError> {
        self.consume_char('"')?;
        let mut out = String::new();
        loop {
            match self.peek_char() {
                Some('"') => {
                    self.advance_char();
                    break;
                }
                Some(ch) => {
                    self.advance_char();
                    out.push(ch);
                }
                None => return Err(self.error_here("Unterminated binary literal")),
            }
        }
        Ok(out)
    }

    fn parse_enum_value(&mut self) -> Result<String, ParseError> {
        self.consume_char('.')?;
        let start = self.snapshot();
        let mut value = String::new();
        while let Some(ch) = self.peek_char() {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                value.push(ch);
                self.advance_char();
            } else {
                break;
            }
        }
        if value.is_empty() {
            return Err(self.error_at(start, "Enumeration value is empty"));
        }
        self.consume_char('.')?;
        Ok(value)
    }

    pub(crate) fn issue_or_error(&mut self, code: &str, message: String) -> Result<(), ParseError> {
        if self.strict {
            return Err(ParseError::new(
                message,
                self.line,
                self.column,
                self.current_line_snippet(),
            ));
        }
        self.warnings.push(ParseWarning {
            code: code.to_string(),
            message,
        });
        Ok(())
    }

    pub(crate) fn push_warning(&mut self, code: &str, message: String) {
        self.warnings.push(ParseWarning {
            code: code.to_string(),
            message,
        });
    }

    fn consume_keyword_ci(&mut self, expected: &str) -> Result<(), ParseError> {
        if self.try_consume_keyword_ci(expected) {
            Ok(())
        } else {
            Err(self.error_here(format!("Expected keyword {expected}")))
        }
    }

    fn try_consume_keyword_ci(&mut self, expected: &str) -> bool {
        let snapshot = self.snapshot();
        self.skip_ws();
        let mut found = String::new();
        while let Some(ch) = self.peek_char() {
            if is_keyword_char(ch) {
                found.push(ch);
                self.advance_char();
            } else {
                break;
            }
        }
        if !found.is_empty() && found.eq_ignore_ascii_case(expected) {
            true
        } else {
            self.restore(snapshot);
            false
        }
    }

    fn consume_literal_ci(&mut self, expected: &str) -> Result<(), ParseError> {
        if self.try_consume_literal_ci(expected) {
            Ok(())
        } else {
            Err(self.error_here(format!("Expected literal {expected}")))
        }
    }

    fn try_consume_literal_ci(&mut self, expected: &str) -> bool {
        let snapshot = self.snapshot();
        for expected_char in expected.chars() {
            let Some(actual) = self.peek_char() else {
                self.restore(snapshot);
                return false;
            };
            if !actual.eq_ignore_ascii_case(&expected_char) {
                self.restore(snapshot);
                return false;
            }
            self.advance_char();
        }
        true
    }

    fn seek_literal_ci(&mut self, expected: &str) -> bool {
        while !self.is_eof() {
            if self.try_consume_literal_ci(expected) {
                return true;
            }
            self.advance_char();
        }
        false
    }

    fn parse_keyword(&mut self) -> Result<String, ParseError> {
        self.skip_ws();
        let start = self.snapshot();
        let mut keyword = String::new();
        while let Some(ch) = self.peek_char() {
            if is_keyword_char(ch) {
                keyword.push(ch);
                self.advance_char();
            } else {
                break;
            }
        }
        if keyword.is_empty() {
            Err(self.error_at(start, "Expected keyword"))
        } else {
            Ok(keyword)
        }
    }

    fn consume_char(&mut self, expected: char) -> Result<(), ParseError> {
        match self.peek_char() {
            Some(ch) if ch == expected => {
                self.advance_char();
                Ok(())
            }
            _ => Err(self.error_here(format!("Expected '{expected}'"))),
        }
    }

    fn try_consume_char(&mut self, expected: char) -> bool {
        if self.peek_char() == Some(expected) {
            self.advance_char();
            true
        } else {
            false
        }
    }

    fn skip_ws(&mut self) {
        while let Some(ch) = self.peek_char() {
            if ch.is_whitespace() {
                self.advance_char();
            } else {
                break;
            }
        }
    }

    fn is_eof(&self) -> bool {
        self.index >= self.chars.len()
    }

    fn peek_char(&self) -> Option<char> {
        self.chars.get(self.index).copied()
    }

    fn peek_next_char(&self) -> Option<char> {
        self.chars.get(self.index + 1).copied()
    }

    fn peek_following_non_ws_char(&self, offset: usize) -> Option<char> {
        let mut idx = self.index + offset;
        while let Some(ch) = self.chars.get(idx).copied() {
            if ch.is_whitespace() {
                idx += 1;
                continue;
            }
            return Some(ch);
        }
        None
    }

    fn advance_char(&mut self) {
        if let Some(ch) = self.peek_char() {
            self.index += 1;
            if ch == '\n' {
                self.line += 1;
                self.column = 1;
            } else {
                self.column += 1;
            }
        }
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            index: self.index,
            line: self.line,
            column: self.column,
        }
    }

    fn restore(&mut self, snapshot: Snapshot) {
        self.index = snapshot.index;
        self.line = snapshot.line;
        self.column = snapshot.column;
    }

    fn error_here(&self, message: impl Into<String>) -> ParseError {
        ParseError::new(message, self.line, self.column, self.current_line_snippet())
    }

    fn error_at(&self, snapshot: Snapshot, message: impl Into<String>) -> ParseError {
        let snippet = self
            .input
            .lines()
            .nth(snapshot.line.saturating_sub(1))
            .unwrap_or("")
            .to_string();
        ParseError::new(message, snapshot.line, snapshot.column, snippet)
    }

    fn current_line_snippet(&self) -> String {
        self.input
            .lines()
            .nth(self.line.saturating_sub(1))
            .unwrap_or("")
            .to_string()
    }
}

impl EntityInstance {
    pub(crate) fn collect_references(&self) -> Vec<i64> {
        let mut refs = Vec::new();
        match &self.body {
            EntityBody::Simple(record) => collect_refs_from_record(record, &mut refs),
            EntityBody::Complex(records) => {
                for record in records {
                    collect_refs_from_record(record, &mut refs);
                }
            }
        }
        refs
    }
}

pub(crate) fn collect_refs_from_record(record: &Record, refs: &mut Vec<i64>) {
    for value in &record.parameters {
        collect_refs_from_value(value, refs);
    }
}

pub(crate) fn collect_refs_from_value(value: &Value, refs: &mut Vec<i64>) {
    match value {
        Value::Reference(id) => refs.push(*id),
        Value::List(values) => {
            for item in values {
                collect_refs_from_value(item, refs);
            }
        }
        Value::Typed { parameters, .. } => {
            for item in parameters {
                collect_refs_from_value(item, refs);
            }
        }
        Value::Unset
        | Value::Omitted
        | Value::Integer(_)
        | Value::Real(_)
        | Value::String(_)
        | Value::Enum(_)
        | Value::Binary(_) => {}
    }
}

pub(crate) fn parse_sfc_attribute_mechanism(
    name: &str,
) -> Result<Option<SfcAttributeMechanism>, String> {
    let (mechanism, remainder) = if let Some(remainder) = name.strip_prefix("$$ATRF$$") {
        ("ATRF", remainder)
    } else if let Some(remainder) = name.strip_prefix("$$ATRU$$") {
        ("ATRU", remainder)
    } else if let Some(remainder) = name.strip_prefix("$$ATRS$$") {
        ("ATRS", remainder)
    } else {
        return Ok(None);
    };

    let fields = remainder.split("$$").collect::<Vec<_>>();
    if fields.iter().any(|field| field.is_empty()) {
        return Err(format!(
            "{mechanism} attribute name contains an empty field; optional trailing fields must be omitted"
        ));
    }

    let field = |index: usize| fields.get(index).map(|value| (*value).to_string());
    let parsed = match mechanism {
        "ATRF" => {
            if !(1..=2).contains(&fields.len()) {
                return Err(format!(
                    "ATRF attribute name requires 1 or 2 fields after the prefix, got {}",
                    fields.len()
                ));
            }
            SfcAttributeMechanism::AttributeFile {
                figure_id: fields[0].to_string(),
                attribute_file_name: field(1),
            }
        }
        "ATRU" => {
            if !(1..=6).contains(&fields.len()) {
                return Err(format!(
                    "ATRU attribute name requires 1 to 6 fields after the prefix, got {}",
                    fields.len()
                ));
            }
            SfcAttributeMechanism::SingleAttribute {
                figure_id: fields[0].to_string(),
                figure_name: field(1),
                attribute_name: field(2),
                attribute_value: field(3),
                attribute_type: field(4),
                unit: field(5),
            }
        }
        "ATRS" => {
            if !(2..=4).contains(&fields.len()) {
                return Err(format!(
                    "ATRS attribute name requires 2 to 4 fields after the prefix, got {}",
                    fields.len()
                ));
            }
            SfcAttributeMechanism::TextAttribute {
                figure_id: fields[0].to_string(),
                attribute_name: fields[1].to_string(),
                attribute_type: field(2),
                unit: field(3),
            }
        }
        _ => unreachable!("mechanism is selected from fixed prefixes"),
    };
    Ok(Some(parsed))
}

pub(crate) fn sfc_attribute_figure_id(mechanism: &SfcAttributeMechanism) -> &str {
    match mechanism {
        SfcAttributeMechanism::AttributeFile { figure_id, .. }
        | SfcAttributeMechanism::SingleAttribute { figure_id, .. }
        | SfcAttributeMechanism::TextAttribute { figure_id, .. } => figure_id,
    }
}

pub(crate) fn resolve_sfc_attribute_file_name(
    document: &ParsedDocument,
    mechanism: &SfcAttributeMechanism,
) -> Option<String> {
    let SfcAttributeMechanism::AttributeFile {
        attribute_file_name,
        ..
    } = mechanism
    else {
        return None;
    };
    if let Some(file_name) = attribute_file_name {
        return Some(file_name.clone());
    }

    let file_name = document.header.find_keyword("FILE_NAME")?;
    let Value::String(drawing_file_name) = file_name.parameters.first()? else {
        return None;
    };
    Some(
        Path::new(drawing_file_name)
            .with_extension("SAF")
            .to_string_lossy()
            .into_owned(),
    )
}

pub(crate) fn is_keyword_start(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch == '_'
}

pub(crate) fn is_keyword_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

pub fn parse_p21_text(input: &str, strict: bool) -> Result<ParseOutput, ParseError> {
    Parser::new(input, FileFormat::P21, strict).parse()
}

pub fn parse_sfc_text(input: &str, strict: bool) -> Result<ParseOutput, ParseError> {
    Parser::new(input, FileFormat::Sfc, strict).parse()
}

pub(crate) fn decode_bytes(
    format: FileFormat,
    bytes: &[u8],
    strict: bool,
) -> Result<(String, Option<ParseWarning>), ParseError> {
    match String::from_utf8(bytes.to_vec()) {
        Ok(text) => Ok((text, None)),
        Err(_) if format == FileFormat::Sfc => {
            let (decoded, had_errors) = SHIFT_JIS.decode_without_bom_handling(bytes);
            if !had_errors {
                return Ok((decoded.into_owned(), None));
            }
            if strict {
                return Err(ParseError::new(
                    "Input is neither valid UTF-8 nor valid Shift-JIS/CP932",
                    1,
                    1,
                    "",
                ));
            }
            Ok((
                decoded.into_owned(),
                Some(ParseWarning {
                    code: "encoding".to_string(),
                    message: "Input contained invalid Shift-JIS/CP932 byte sequences and was decoded with replacement characters."
                        .to_string(),
                }),
            ))
        }
        Err(_) => Err(ParseError::new("Input is not valid UTF-8", 1, 1, "")),
    }
}

pub(crate) fn parse_from_bytes(
    format: FileFormat,
    bytes: &[u8],
    strict: bool,
) -> Result<ParseOutput, ParseError> {
    let (text, encoding_warning) = decode_bytes(format, bytes, strict)?;
    let mut output = match format {
        FileFormat::P21 => parse_p21_text(&text, strict)?,
        FileFormat::Sfc => parse_sfc_text(&text, strict)?,
    };
    if let Some(warning) = encoding_warning {
        output.warnings.push(warning);
    }
    Ok(output)
}

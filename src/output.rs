use crate::prelude::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OutputFormat {
    Json,
    CompactJson,
    Jsonl,
    Csv,
    Tsv,
    Table,
}

impl OutputFormat {
    pub(crate) fn parse(value: &str) -> Result<Self> {
        match value {
            "json" => Ok(Self::Json),
            "compact-json" => Ok(Self::CompactJson),
            "jsonl" => Ok(Self::Jsonl),
            "csv" => Ok(Self::Csv),
            "tsv" => Ok(Self::Tsv),
            "table" => Ok(Self::Table),
            other => err(format!(
                "--format must be json, compact-json, jsonl, csv, tsv, or table; got {other}"
            )),
        }
    }
}

pub(crate) fn write_value(matches: &ArgMatches, data: Value) -> Result<()> {
    let format = output_format_from_matches(matches)?;
    let output = render_value(data, format)?;
    if let Some(path) = matches.get_one::<String>("output") {
        fs::write(path, output)
            .into_diagnostic()
            .wrap_err_with(|| format!("writing {path}"))?;
    } else {
        print_output(&output);
    }
    Ok(())
}

pub(crate) fn write_value_stdout(matches: &ArgMatches, data: Value) -> Result<()> {
    let format = output_format_from_matches(matches)?;
    let output = render_value(data, format)?;
    print_output(&output);
    Ok(())
}

fn print_output(output: &str) {
    print!("{output}");
    if !output.ends_with('\n') {
        println!();
    }
}

pub(crate) fn output_format_from_matches(matches: &ArgMatches) -> Result<OutputFormat> {
    OutputFormat::parse(
        matches
            .get_one::<String>("format")
            .map(String::as_str)
            .unwrap_or("json"),
    )
}

pub(crate) fn render_value(data: Value, format: OutputFormat) -> Result<String> {
    match format {
        OutputFormat::Json => {
            serde_json::to_string_pretty(&data)
                .into_diagnostic()
                .map(|mut text| {
                    text.push('\n');
                    text
                })
        }
        OutputFormat::CompactJson => {
            serde_json::to_string(&data)
                .into_diagnostic()
                .map(|mut text| {
                    text.push('\n');
                    text
                })
        }
        OutputFormat::Jsonl => render_jsonl(&data),
        OutputFormat::Csv => render_delimited(&data, b','),
        OutputFormat::Tsv => render_delimited(&data, b'\t'),
        OutputFormat::Table => Ok(render_table(&data)),
    }
}

pub(crate) fn render_jsonl(data: &Value) -> Result<String> {
    let rows = rows_from_value(data);
    if rows.is_empty() {
        return serde_json::to_string(data)
            .into_diagnostic()
            .map(|mut text| {
                text.push('\n');
                text
            });
    }
    let mut output = String::new();
    for row in rows {
        output.push_str(&serde_json::to_string(&Value::Object(row)).into_diagnostic()?);
        output.push('\n');
    }
    Ok(output)
}

pub(crate) fn render_delimited(data: &Value, delimiter: u8) -> Result<String> {
    let rows = rows_from_value(data);
    if rows.is_empty() {
        return Ok(String::new());
    }
    let headers = headers_for_rows(&rows);
    let mut writer = csv::WriterBuilder::new()
        .delimiter(delimiter)
        .from_writer(Vec::new());
    writer.write_record(&headers).into_diagnostic()?;
    for row in rows {
        let record = headers
            .iter()
            .map(|header| row.get(header).map(cell_string).unwrap_or_default())
            .collect::<Vec<_>>();
        writer.write_record(record).into_diagnostic()?;
    }
    let bytes = writer.into_inner().into_diagnostic()?;
    String::from_utf8(bytes).into_diagnostic()
}

pub(crate) fn render_table(data: &Value) -> String {
    let rows = rows_from_value(data);
    if rows.is_empty() {
        return serde_json::to_string_pretty(data).unwrap_or_else(|_| data.to_string());
    }
    let headers = headers_for_rows(&rows);
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(headers.iter().map(Cell::new).collect::<Vec<_>>());
    for row in rows {
        table.add_row(
            headers
                .iter()
                .map(|header| Cell::new(row.get(header).map(cell_string).unwrap_or_default()))
                .collect::<Vec<_>>(),
        );
    }
    let mut output = table.to_string();
    output.push('\n');
    output
}

pub(crate) fn rows_from_value(data: &Value) -> Vec<Map<String, Value>> {
    match data {
        Value::Array(items) => items.iter().filter_map(value_as_row).collect(),
        Value::Object(object) => {
            for key in ["results", "contacts", "groups", "items", "actions", "data"] {
                if let Some(Value::Array(items)) = object.get(key) {
                    let rows = items.iter().filter_map(value_as_row).collect::<Vec<_>>();
                    if !rows.is_empty() {
                        return rows;
                    }
                }
            }
            vec![object.clone()]
        }
        _ => Vec::new(),
    }
}

pub(crate) fn value_as_row(value: &Value) -> Option<Map<String, Value>> {
    match value {
        Value::Object(object) => Some(object.clone()),
        _ => None,
    }
}

pub(crate) fn headers_for_rows(rows: &[Map<String, Value>]) -> Vec<String> {
    let mut headers = BTreeSet::new();
    for row in rows {
        for key in row.keys() {
            headers.insert(key.clone());
        }
    }
    headers.into_iter().collect()
}

pub(crate) fn cell_string(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        Value::String(value) => value.clone(),
        Value::Array(_) | Value::Object(_) => serde_json::to_string(value).unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_format_parse_accepts_documented_formats() {
        assert_eq!(OutputFormat::parse("json").unwrap(), OutputFormat::Json);
        assert_eq!(
            OutputFormat::parse("compact-json").unwrap(),
            OutputFormat::CompactJson
        );
        assert_eq!(OutputFormat::parse("jsonl").unwrap(), OutputFormat::Jsonl);
        assert_eq!(OutputFormat::parse("csv").unwrap(), OutputFormat::Csv);
        assert_eq!(OutputFormat::parse("tsv").unwrap(), OutputFormat::Tsv);
        assert_eq!(OutputFormat::parse("table").unwrap(), OutputFormat::Table);
    }

    #[test]
    fn output_format_parse_rejects_unknown_formats() {
        let error = OutputFormat::parse("yaml").unwrap_err().to_string();
        assert!(error.contains("--format must be json, compact-json, jsonl, csv, tsv, or table"));
        assert!(error.contains("yaml"));
    }

    #[test]
    fn rows_from_value_extracts_first_non_empty_known_collection() {
        let value = json!({
            "results": ["ignored"],
            "contacts": [{"id": "contact-1"}],
            "data": [{"id": "data-1"}]
        });

        let rows = rows_from_value(&value);

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].get("id"), Some(&json!("contact-1")));
    }

    #[test]
    fn rows_from_value_falls_back_to_root_object() {
        let value = json!({
            "results": [null, 42],
            "ok": true
        });

        let rows = rows_from_value(&value);

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].get("ok"), Some(&json!(true)));
        assert!(rows[0].contains_key("results"));
    }

    #[test]
    fn headers_for_rows_merges_and_sorts_keys() {
        let rows = vec![
            json!({"z": 1, "a": 2}).as_object().unwrap().clone(),
            json!({"middle": 3, "a": 4}).as_object().unwrap().clone(),
        ];

        assert_eq!(
            headers_for_rows(&rows),
            vec!["a".to_string(), "middle".to_string(), "z".to_string()]
        );
    }

    #[test]
    fn render_jsonl_emits_one_object_per_row() -> Result<()> {
        let output = render_jsonl(&json!({
            "results": [
                {"b": 2, "a": 1},
                42,
                {"a": 3}
            ]
        }))?;

        let rows = output
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .collect::<Vec<_>>();

        assert_eq!(rows, vec![json!({"a": 1, "b": 2}), json!({"a": 3})]);
        Ok(())
    }

    #[test]
    fn render_jsonl_serializes_non_row_values() -> Result<()> {
        assert_eq!(render_jsonl(&json!("text"))?, "\"text\"\n");
        Ok(())
    }

    #[test]
    fn render_delimited_uses_sorted_headers() -> Result<()> {
        let output = render_delimited(
            &json!([
                {"b": 2, "a": "x"},
                {"a": "y", "c": true}
            ]),
            b',',
        )?;

        assert_eq!(
            output.lines().collect::<Vec<_>>(),
            vec!["a,b,c", "x,2,", "y,,true"]
        );
        Ok(())
    }

    #[test]
    fn render_delimited_can_write_tsv() -> Result<()> {
        let output = render_delimited(&json!([{"b": 2, "a": "x"}]), b'\t')?;

        assert_eq!(output.lines().collect::<Vec<_>>(), vec!["a\tb", "x\t2"]);
        Ok(())
    }

    #[test]
    fn cell_string_serializes_supported_cell_values() {
        assert_eq!(cell_string(&Value::Null), "");
        assert_eq!(cell_string(&json!(true)), "true");
        assert_eq!(cell_string(&json!(42)), "42");
        assert_eq!(cell_string(&json!("mesh")), "mesh");
        assert_eq!(cell_string(&json!([1, 2])), "[1,2]");
        assert_eq!(cell_string(&json!({"nested": true})), "{\"nested\":true}");
    }
}

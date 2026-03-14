use std::collections::HashMap;
use std::path::Path;

use crate::types::AnnotationValue;

/// A parsed rule from a CSV row (not yet compiled into a regex).
#[derive(Debug, Clone)]
pub struct CsvRule {
    pub pattern: String,
    pub attributes: HashMap<String, AnnotationValue>,
    pub group: u32,
    pub priority: i32,
}

/// Supported column types for CSV loading.
/// Maps to EstNLTK's `CONVERSION_MAP` (minus `callable`, `expression`, `regex`
/// which are Python-specific).
#[derive(Debug, Clone, Copy)]
enum ColumnType {
    Str,
    Int,
    Float,
    Bool,
}

impl ColumnType {
    fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "string" => Ok(ColumnType::Str),
            "int" => Ok(ColumnType::Int),
            "float" => Ok(ColumnType::Float),
            "bool" => Ok(ColumnType::Bool),
            other => Err(format!(
                "Unknown data type '{}'. Supported types: string, int, float, bool",
                other
            )),
        }
    }

    fn convert(&self, value: &str, line: usize, col: &str) -> Result<AnnotationValue, String> {
        match self {
            ColumnType::Str => Ok(AnnotationValue::Str(value.to_string())),
            ColumnType::Int => value
                .parse::<i64>()
                .map(AnnotationValue::Int)
                .map_err(|_| {
                    format!(
                        "Line {}: cannot convert '{}' to int in column '{}'",
                        line, value, col
                    )
                }),
            ColumnType::Float => value
                .parse::<f64>()
                .map(AnnotationValue::Float)
                .map_err(|_| {
                    format!(
                        "Line {}: cannot convert '{}' to float in column '{}'",
                        line, value, col
                    )
                }),
            ColumnType::Bool => match value {
                "true" | "True" | "TRUE" | "1" => Ok(AnnotationValue::Bool(true)),
                "false" | "False" | "FALSE" | "0" => Ok(AnnotationValue::Bool(false)),
                _ => Err(format!(
                    "Line {}: cannot convert '{}' to bool in column '{}'",
                    line, value, col
                )),
            },
        }
    }
}

/// Resolve a column reference (name or index) to a column index.
fn resolve_column(
    col: &ColumnRef,
    column_names: &[String],
    label: &str,
) -> Result<usize, String> {
    match col {
        ColumnRef::Index(i) => {
            if *i >= column_names.len() {
                return Err(format!("{} column index {} is out of range", label, i));
            }
            Ok(*i)
        }
        ColumnRef::Name(name) => column_names
            .iter()
            .position(|n| n == name)
            .ok_or_else(|| {
                format!(
                    "{} column '{}' is missing from the file header",
                    label, name
                )
            }),
    }
}

/// Reference to a CSV column: by index or by name.
#[derive(Debug, Clone)]
pub enum ColumnRef {
    Index(usize),
    Name(String),
}

/// Configuration for CSV loading.
#[derive(Debug)]
pub struct CsvLoadConfig {
    /// Which column contains the pattern (default: index 0).
    pub key_column: ColumnRef,
    /// Optional column for the group attribute.
    pub group_column: Option<ColumnRef>,
    /// Optional column for the priority attribute.
    pub priority_column: Option<ColumnRef>,
}

impl Default for CsvLoadConfig {
    fn default() -> Self {
        Self {
            key_column: ColumnRef::Index(0),
            group_column: None,
            priority_column: None,
        }
    }
}

/// Load extraction rules from a CSV file.
///
/// CSV format (matching EstNLTK's `AmbiguousRuleset.load()`):
/// - Row 1: column names (attribute names)
/// - Row 2: column types (`string`, `int`, `float`, `bool`)
/// - Row 3+: data rows
///
/// The key column contains the regex/substring pattern.
/// Optional group and priority columns are excluded from attributes.
pub fn load_rules_from_csv<P: AsRef<Path>>(
    path: P,
    config: &CsvLoadConfig,
) -> Result<Vec<CsvRule>, String> {
    let path = path.as_ref();
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(false)
        .from_path(path)
        .map_err(|e| format!("Cannot open '{}': {}", path.display(), e))?;

    let mut records = rdr.records();

    // Row 1: column names
    let column_names: Vec<String> = records
        .next()
        .ok_or("Invalid file format: Line 1: The first header row is missing")?
        .map_err(|e| format!("Invalid file format: Line 1: {}", e))?
        .iter()
        .map(|s| s.to_string())
        .collect();

    if column_names.is_empty() {
        return Err("Invalid file format: Line 1: No columns found".to_string());
    }

    // Row 2: column types
    let type_record = records
        .next()
        .ok_or("Invalid file format: Line 2: The second header row is missing")?
        .map_err(|e| format!("Invalid file format: Line 2: {}", e))?;

    let type_strings: Vec<String> = type_record.iter().map(|s| s.to_string()).collect();

    if type_strings.len() != column_names.len() {
        return Err("Invalid file format: Line 2: Header rows have different length".to_string());
    }

    let n = column_names.len();

    // Parse column types
    let mut converters = Vec::with_capacity(n);
    for (i, ts) in type_strings.iter().enumerate() {
        let ct = ColumnType::from_str(ts).map_err(|e| {
            format!(
                "Invalid file format: Line 2: column '{}': {}",
                column_names[i], e
            )
        })?;
        converters.push(ct);
    }

    // Resolve column indices
    let key_idx = resolve_column(&config.key_column, &column_names, "Key")?;

    let group_idx = match &config.group_column {
        Some(c) => {
            let idx = resolve_column(c, &column_names, "Group")?;
            if idx == key_idx {
                return Err("Group column cannot coincide with key column".to_string());
            }
            Some(idx)
        }
        None => None,
    };

    let priority_idx = match &config.priority_column {
        Some(c) => {
            let idx = resolve_column(c, &column_names, "Priority")?;
            if idx == key_idx {
                return Err("Priority column cannot coincide with key column".to_string());
            }
            if group_idx == Some(idx) {
                return Err("Priority column cannot coincide with group column".to_string());
            }
            Some(idx)
        }
        None => None,
    };

    // Columns that are not attributes
    let non_attr: Vec<usize> = [Some(key_idx), group_idx, priority_idx]
        .iter()
        .filter_map(|&x| x)
        .collect();

    // Row 3+: data rows
    let mut rules = Vec::new();
    for (row_num, record_result) in records.enumerate() {
        let line = row_num + 3; // 1-indexed, after 2 header rows
        let record = record_result
            .map_err(|e| format!("Invalid file format: Line {}: {}", line, e))?;

        let fields: Vec<&str> = record.iter().collect();
        if fields.len() != n {
            return Err(format!(
                "Invalid file format: Line {}: expected {} columns, got {}",
                line,
                n,
                fields.len()
            ));
        }

        // Extract pattern
        let pattern = fields[key_idx].to_string();

        // Extract group
        let group: u32 = match group_idx {
            Some(gi) => {
                let val = converters[gi].convert(fields[gi], line, &column_names[gi])?;
                match val {
                    AnnotationValue::Int(i) => {
                        if i < 0 {
                            return Err(format!(
                                "Line {}: group value must be non-negative, got {}",
                                line, i
                            ));
                        }
                        i as u32
                    }
                    _ => {
                        return Err(format!(
                            "Line {}: group column must be of type int",
                            line
                        ))
                    }
                }
            }
            None => 0,
        };

        // Extract priority
        let priority: i32 = match priority_idx {
            Some(pi) => {
                let val = converters[pi].convert(fields[pi], line, &column_names[pi])?;
                match val {
                    AnnotationValue::Int(i) => i as i32,
                    _ => {
                        return Err(format!(
                            "Line {}: priority column must be of type int",
                            line
                        ))
                    }
                }
            }
            None => 0,
        };

        // Build attributes from remaining columns
        let mut attributes = HashMap::new();
        for i in 0..n {
            if non_attr.contains(&i) {
                continue;
            }
            let val = converters[i].convert(fields[i], line, &column_names[i])?;
            attributes.insert(column_names[i].clone(), val);
        }

        rules.push(CsvRule {
            pattern,
            attributes,
            group,
            priority,
        });
    }

    Ok(rules)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_temp_csv(content: &str) -> tempfile::NamedTempFile {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f.flush().unwrap();
        f
    }

    #[test]
    fn test_basic_csv_loading() {
        let csv = "pattern,type\nstring,string\n[0-9]+,number\n[a-z]+,word\n";
        let f = write_temp_csv(csv);
        let rules = load_rules_from_csv(f.path(), &CsvLoadConfig::default()).unwrap();
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].pattern, "[0-9]+");
        assert_eq!(
            rules[0].attributes.get("type"),
            Some(&AnnotationValue::Str("number".to_string()))
        );
        assert_eq!(rules[1].pattern, "[a-z]+");
        assert_eq!(
            rules[1].attributes.get("type"),
            Some(&AnnotationValue::Str("word".to_string()))
        );
    }

    #[test]
    fn test_csv_with_priority_and_group() {
        let csv = "pattern,group,priority,label\nstring,int,int,string\nhello,0,1,greeting\nworld,0,2,noun\n";
        let f = write_temp_csv(csv);
        let config = CsvLoadConfig {
            key_column: ColumnRef::Index(0),
            group_column: Some(ColumnRef::Name("group".to_string())),
            priority_column: Some(ColumnRef::Name("priority".to_string())),
        };
        let rules = load_rules_from_csv(f.path(), &config).unwrap();
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].pattern, "hello");
        assert_eq!(rules[0].group, 0);
        assert_eq!(rules[0].priority, 1);
        assert_eq!(
            rules[0].attributes.get("label"),
            Some(&AnnotationValue::Str("greeting".to_string()))
        );
        // group and priority should NOT be in attributes
        assert!(rules[0].attributes.get("group").is_none());
        assert!(rules[0].attributes.get("priority").is_none());
    }

    #[test]
    fn test_csv_int_float_bool_types() {
        let csv = "pattern,count,weight,active\nstring,int,float,bool\nabc,42,3.14,true\ndef,0,0.0,false\n";
        let f = write_temp_csv(csv);
        let rules = load_rules_from_csv(f.path(), &CsvLoadConfig::default()).unwrap();
        assert_eq!(rules.len(), 2);
        assert_eq!(
            rules[0].attributes.get("count"),
            Some(&AnnotationValue::Int(42))
        );
        assert_eq!(
            rules[0].attributes.get("weight"),
            Some(&AnnotationValue::Float(3.14))
        );
        assert_eq!(
            rules[0].attributes.get("active"),
            Some(&AnnotationValue::Bool(true))
        );
        assert_eq!(
            rules[1].attributes.get("active"),
            Some(&AnnotationValue::Bool(false))
        );
    }

    #[test]
    fn test_csv_key_column_by_name() {
        let csv = "label,regex_pattern,count\nstring,string,int\ngreeting,hello,5\n";
        let f = write_temp_csv(csv);
        let config = CsvLoadConfig {
            key_column: ColumnRef::Name("regex_pattern".to_string()),
            group_column: None,
            priority_column: None,
        };
        let rules = load_rules_from_csv(f.path(), &config).unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].pattern, "hello");
        assert_eq!(
            rules[0].attributes.get("label"),
            Some(&AnnotationValue::Str("greeting".to_string()))
        );
        assert_eq!(
            rules[0].attributes.get("count"),
            Some(&AnnotationValue::Int(5))
        );
        // pattern column excluded from attributes
        assert!(rules[0].attributes.get("regex_pattern").is_none());
    }

    #[test]
    fn test_csv_missing_header_row() {
        let csv = "";
        let f = write_temp_csv(csv);
        let result = load_rules_from_csv(f.path(), &CsvLoadConfig::default());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("first header row"));
    }

    #[test]
    fn test_csv_missing_type_row() {
        let csv = "pattern,type\n";
        let f = write_temp_csv(csv);
        let result = load_rules_from_csv(f.path(), &CsvLoadConfig::default());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("second header row"));
    }

    #[test]
    fn test_csv_unknown_type() {
        let csv = "pattern,label\nstring,callable\nhello,greet\n";
        let f = write_temp_csv(csv);
        let result = load_rules_from_csv(f.path(), &CsvLoadConfig::default());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown data type"));
    }

    #[test]
    fn test_csv_wrong_column_count() {
        let csv = "pattern,type\nstring,string\nhello\n";
        let f = write_temp_csv(csv);
        let result = load_rules_from_csv(f.path(), &CsvLoadConfig::default());
        assert!(result.is_err());
        let err = result.unwrap_err();
        // csv crate may report this as a parse error or we catch it as column count mismatch
        assert!(
            err.contains("expected 2 columns") || err.contains("Line 3"),
            "Unexpected error: {}",
            err
        );
    }

    #[test]
    fn test_csv_key_column_out_of_range() {
        let csv = "pattern\nstring\nhello\n";
        let f = write_temp_csv(csv);
        let config = CsvLoadConfig {
            key_column: ColumnRef::Index(5),
            group_column: None,
            priority_column: None,
        };
        let result = load_rules_from_csv(f.path(), &config);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("out of range"));
    }

    #[test]
    fn test_csv_group_coincides_with_key() {
        let csv = "pattern,label\nstring,string\nhello,test\n";
        let f = write_temp_csv(csv);
        let config = CsvLoadConfig {
            key_column: ColumnRef::Index(0),
            group_column: Some(ColumnRef::Index(0)),
            priority_column: None,
        };
        let result = load_rules_from_csv(f.path(), &config);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("coincide"));
    }
}

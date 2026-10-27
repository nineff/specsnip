use std::str::from_utf8;

#[cfg(test)]
use serde::Deserialize;
use serde::Serialize;
use tree_sitter::{Node, Parser, Query, QueryCursor, StreamingIterator, Tree};
#[cfg(target_arch = "wasm32")]
use wasm_minimal_protocol::{initiate_protocol, wasm_func};

#[cfg(target_arch = "wasm32")]
initiate_protocol!();

const C_KEYWORDS: [&str; 32] = [
    "auto", "break", "case", "char", "const", "continue", "default", "do", "double", "else",
    "enum", "extern", "float", "for", "goto", "if", "int", "long", "register", "return", "short",
    "signed", "sizeof", "static", "struct", "switch", "typedef", "union", "unsigned", "void",
    "volatile", "while",
];

#[derive(Debug, Serialize, PartialEq)]
struct LineRange {
    start: usize,
    end: usize,
}

#[derive(Debug, Serialize)]
struct SerializeableTreeNode {
    kind: String,
    start_byte: usize,
    end_byte: usize,
    start_position: (usize, usize),
    end_position: (usize, usize),
    text: Option<String>,
    children: Vec<SerializeableTreeNode>,
}

#[derive(Debug, Serialize)]
#[cfg_attr(test, derive(Deserialize))]
pub struct QueryMatchResult {
    pub pattern_index: usize,
    pub captures: Vec<CaptureResult>,
}

#[derive(Debug, Serialize)]
#[cfg_attr(test, derive(Deserialize))]
pub struct CaptureResult {
    pub name: String,
    pub kind: String,
    pub start_byte: usize,
    pub end_byte: usize,
    pub text: String,
}

fn walk_tree_recursive(node: Node, source: &[u8]) -> Result<SerializeableTreeNode, String> {
    let mut cursor = node.walk();

    let children = node
        .children(&mut cursor)
        .map(|child| walk_tree_recursive(child, source))
        .collect::<Result<Vec<_>, String>>()?;

    Ok(SerializeableTreeNode {
        kind: node.kind().to_owned(),
        start_byte: node.start_byte(),
        end_byte: node.end_byte(),
        start_position: (node.start_position().row, node.start_position().column),
        end_position: (node.end_position().row, node.end_position().column),
        text: if node.child_count() == 0 {
            Some(
                node.utf8_text(source)
                    .map_err(|e| format!("Source contains invalid UTF: {e}"))?
                    .to_owned(),
            )
        } else {
            None
        },
        children,
    })
}

#[cfg_attr(target_arch = "wasm32", wasm_func)]
pub fn check_identifier_existance(code: &[u8], identifier: &[u8]) -> Result<Vec<u8>, String> {
    let tree = parse_sourecode(code)?;
    let root_node = tree.root_node();

    let name = parse_identifier(identifier)?;

    let query = Query::new(
        &tree_sitter_c::LANGUAGE.into(),
        &format!(
            "((identifier) @name
        (#eq? @name \"{name}\"))"
        ),
    )
    .map_err(|e| format!("unable to build query: {e}"))?;

    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, root_node, code);
    if matches.next().is_some() {
        serialize_to_cbor(true)
    } else {
        serialize_to_cbor(false)
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_func)]
pub fn get_type_definition_line_range(code: &[u8], type_name: &[u8]) -> Result<Vec<u8>, String> {
    let tree = parse_sourecode(code)?;
    let root_node = tree.root_node();

    serialize_to_cbor(query_type_definition_linerange(code, type_name, root_node)?)
}

#[cfg_attr(target_arch = "wasm32", wasm_func)]
pub fn get_function_definition_line_range(
    code: &[u8],
    function_name: &[u8],
) -> Result<Vec<u8>, String> {
    let tree = parse_sourecode(code)?;
    let root_node = tree.root_node();

    serialize_to_cbor(query_function_definition_linerange(
        code,
        function_name,
        root_node,
    )?)
}
#[cfg_attr(target_arch = "wasm32", wasm_func)]
pub fn query_tree(code: &[u8], query: &[u8]) -> Result<Vec<u8>, String> {
    let tree = parse_sourecode(code)?;
    let query = from_utf8(query).map_err(|e| format!("Provided query is not valid UTF-8: {e}"))?;
    let root_node = tree.root_node();
    let query = Query::new(&tree_sitter_c::LANGUAGE.into(), query)
        .map_err(|e| format!("unable to build query: {e}"))?;

    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, root_node, code);
    let mut results = Vec::new();

    while let Some(m) = matches.next() {
        let captures = m
            .captures()
            .iter()
            .map(|capture| {
                let node = capture.node;
                let range = node.byte_range();

                CaptureResult {
                    name: query.capture_names()[capture.index as usize].to_owned(),
                    kind: node.kind().to_owned(),
                    start_byte: range.start,
                    end_byte: range.end,
                    text: from_utf8(code).expect(
                        "code is valid at this point sice it has already been parsed to a tree",
                    )[range.start..range.end]
                        .to_owned(),
                }
            })
            .collect();

        results.push(QueryMatchResult {
            pattern_index: m.pattern_index,
            captures,
        });
    }
    serialize_to_cbor(results)
}

#[cfg_attr(target_arch = "wasm32", wasm_func)]
pub fn get_tree(code: &[u8]) -> Result<Vec<u8>, String> {
    let tree = parse_sourecode(code)?;
    let root_node = tree.root_node();
    let serialized = walk_tree_recursive(root_node, code)?;
    serialize_to_cbor(serialized)
}

fn serialize_to_cbor<T: Serialize>(r: T) -> Result<Vec<u8>, String> {
    let mut buf = Vec::new();
    ciborium::ser::into_writer(&r, &mut buf).map_err(|e| format!("could not serialize: {e}"))?;
    Ok(buf)
}

fn query_function_definition_linerange(
    code: &[u8],
    function_name: &[u8],
    root_node: Node<'_>,
) -> Result<LineRange, String> {
    let function_name = parse_identifier(function_name)?;

    let query = Query::new(
        &tree_sitter_c::LANGUAGE.into(),
        &format!(
            "((declaration
                declarator: (function_declarator
                declarator: (identifier) @function.name
            )) @definition
            (#eq? @function.name \"{function_name}\"))"
        ),
    )
    .map_err(|e| format!("unable to build query: {e}"))?;

    get_query_line_range_with_preeceeding_comments(code, root_node, &query)
        .ok_or_else(|| format!("type definition not found: {function_name}"))
}

fn get_query_line_range_with_preeceeding_comments(
    code: &[u8],
    root_node: Node<'_>,
    query: &Query,
) -> Option<LineRange> {
    let definition_idx = query
        .capture_index_for_name("definition")
        .expect("query contains @definition");
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(query, root_node, code);

    let mut line_range = None;

    // There might be more than one typedef with the given name; use only the last one.
    while let Some(query_match) = matches.next() {
        let Some(capture) = query_match
            .captures()
            .iter()
            .find(|capture| capture.index == definition_idx)
        else {
            continue;
        };

        // Include all consecutive comments immediately preceding the definition.
        let mut start_row = capture.node.start_position().row;
        let mut sibling = capture.node.prev_named_sibling();

        while let Some(node) = sibling {
            if node.kind() != "comment" || node.end_position().row + 1 != start_row {
                break;
            }

            start_row = node.start_position().row;
            sibling = node.prev_named_sibling();
        }

        line_range = Some(LineRange {
            start: start_row + 1,
            end: capture.node.end_position().row + 1,
        });
    }
    line_range
}

fn parse_identifier(identifier: &[u8]) -> Result<&str, String> {
    let identifier = from_utf8(identifier)
        .map_err(|e| format!("Provided identifier is not valid UTF-8: {e}"))?;
    is_valid_c_identifier(identifier).ok_or(format!(
        "Provided string is not a valid C identifier: {identifier}"
    ))?;
    Ok(identifier)
}

fn query_type_definition_linerange(
    code: &[u8],
    type_name: &[u8],
    root_node: Node<'_>,
) -> Result<LineRange, String> {
    let type_name = parse_identifier(type_name)?;

    let query = Query::new(
        &tree_sitter_c::LANGUAGE.into(),
        &format!(
            "((type_definition (type_identifier) @type.name) @definition (#eq? @type.name \"{type_name}\"))"
        ),
    )
    .map_err(|e| format!("unable to build query: {e}"))?;

    get_query_line_range_with_preeceeding_comments(code, root_node, &query)
        .ok_or_else(|| format!("type definition not found: {type_name}"))
}

fn parse_sourecode(code: &[u8]) -> Result<Tree, String> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_c::LANGUAGE.into())
        .map_err(|e| format!("Could not load Tree-Sitter C language: {e}"))?;

    let source_code =
        from_utf8(code).map_err(|e| format!("Provided code is not valid UTF-8: {e}"))?;
    let tree = parser
        .parse(source_code, None)
        .expect("language has been set");
    Ok(tree)
}

//C Identifiers can contain uppercase and lowercase alphabets (A–Z, a–z), digits (0–9), and the underscore (_).
//The first character of an identifier must be a letter or an underscore.
//C Identifiers cannot be keywords in C
fn is_valid_c_identifier(name: &str) -> bool {
    if C_KEYWORDS.contains(&name) {
        return false;
    }
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c == '_' || c.is_ascii_alphabetic())
        && chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
}

#[cfg(test)]
mod tests {

    use std::io::Cursor;

    use ciborium::from_reader;

    use super::*;

    #[test]
    fn test_valid_c_identifiers_are_reported_as_such() {
        for identifier in [
            "foo",
            "_private",
            "CamelCase",
            "snake_case",
            "value123",
            "_",
            "a1",
        ] {
            assert!(
                is_valid_c_identifier(identifier),
                "Expected {identifier:?} to be valid"
            );
        }
    }

    #[test]
    fn test_invalid_c_identifiers_are_reported_as_such() {
        for identifier in [
            "", "5", "123abc", "foo-bar", "foo bar", "foo.bar", "int", "return", "while", "über",
        ] {
            assert!(
                !is_valid_c_identifier(identifier),
                "Expected {identifier:?} to be invalid"
            );
        }
    }
    #[test]
    fn test_invalid_c_identifiers_cause_early_return() {
        let code = "code".as_bytes();
        let tree = parse_sourecode(code).expect("tree can be constructed");
        assert_eq!(
            query_type_definition_linerange(code, "5".as_bytes(), tree.root_node()),
            Err("Provided string is not a valid C identifier: 5".into())
        );
        assert_eq!(
            query_type_definition_linerange(code, "auto".as_bytes(), tree.root_node()),
            Err("Provided string is not a valid C identifier: auto".into())
        );
        assert_eq!(
            query_type_definition_linerange(code, "foo bar".as_bytes(), tree.root_node()),
            Err("Provided string is not a valid C identifier: foo bar".into())
        );
    }

    #[test]
    fn test_invalid_utf8_identifier_returns_error() {
        let identifier = [0xff, 0xfe];

        let result = parse_identifier(&identifier);

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .starts_with("Provided identifier is not valid UTF-8:")
        );
    }

    #[test]
    fn test_invalid_utf8_source_returns_error() {
        let code = [0xff, 0xfe];

        let result = parse_sourecode(&code);

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .starts_with("Provided code is not valid UTF-8:")
        );
    }

    #[test]
    fn test_missing_type_definition_returns_error() {
        let code = b"typedef int ExistingType;\n";
        let tree = parse_sourecode(code).unwrap();

        assert_eq!(
            query_type_definition_linerange(code, b"MissingType", tree.root_node()),
            Err("type definition not found: MissingType".into())
        );
    }

    #[test]
    fn test_missing_function_definition_returns_error() {
        let code = b"int existing_function(void);\n";
        let tree = parse_sourecode(code).unwrap();

        assert_eq!(
            query_function_definition_linerange(code, b"missing_function", tree.root_node()),
            Err("type definition not found: missing_function".into())
        );
    }

    #[test]
    fn test_type_definition_includes_preceding_comments() {
        let code = b"// First comment\n// Second comment\ntypedef int Count;\n";
        let tree = parse_sourecode(code).unwrap();

        assert_eq!(
            query_type_definition_linerange(code, b"Count", tree.root_node()),
            Ok(LineRange { start: 1, end: 3 })
        );
    }

    #[test]
    fn test_type_definition_does_not_include_separated_comments() {
        let code = b"// Unrelated comment\n\n typedef int Count;\n";
        let tree = parse_sourecode(code).unwrap();

        assert_eq!(
            query_type_definition_linerange(code, b"Count", tree.root_node()),
            Ok(LineRange { start: 3, end: 3 })
        );
    }

    #[test]
    fn test_query_tree_rejects_invalid_utf8_query() {
        let code = b"int main(void) { return 0; }\n";
        let query = [0xff];

        let result = query_tree(code, &query);

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .starts_with("Provided query is not valid UTF-8:")
        );
    }

    #[test]
    fn test_query_tree_rejects_invalid_query_syntax() {
        let code = b"int main(void) { return 0; }\n";

        let result = query_tree(code, b"((invalid");

        assert!(result.is_err());
        assert!(result.unwrap_err().starts_with("unable to build query:"));
    }

    #[test]
    fn test_query_captures_function_identifier() {
        let code = b"int main(void) { return 0; }\n";
        let query = b"(function_definition declarator: (function_declarator declarator: (identifier) @name))";

        let result = query_tree(code, query);

        assert!(result.is_ok());

        let cursor = Cursor::new(result.unwrap());
        let data: Vec<QueryMatchResult> = from_reader(cursor).unwrap();

        assert_eq!(data.len(), 1, "Query has only 1 result");
        assert_eq!(
            data[0].pattern_index, 0,
            "there is only one patternindex with value 0"
        );
        assert_eq!(
            data[0].captures.len(),
            1,
            "Query has exactly 1 capture result"
        );
        assert_eq!(&data[0].captures[0].name, "name");
        assert_eq!(&data[0].captures[0].kind, "identifier");
        assert_eq!(&data[0].captures[0].start_byte, &4);
        assert_eq!(&data[0].captures[0].end_byte, &8);
        assert_eq!(&data[0].captures[0].text, "main");
    }

    #[test]
    fn test_identifier_existance() {
        let code = b"int main(void) { return 0; }\n";
        let res = check_identifier_existance(code, b"identifier");
        assert_eq!(res, serialize_to_cbor(false));

        let res = check_identifier_existance(code, b"main");
        assert_eq!(res, serialize_to_cbor(true));
    }

    #[test]
    fn test_example_code_yields_correct_ranges() {
        let code = b"#ifndef VERSION_H
#define VERSION_H

/* Describes how a version should be interpreted or used. */
typedef enum {
    MODE_RELEASE = 0,  /* Stable, production-ready version. */
    MODE_DEBUG   = 1,  /* Debug/development version. */
    MODE_BETA    = 2   /* Pre-release testing version. */
} VersionMode;

/* Represents a semantic-style version: major.minor.patch. */
typedef struct {
    int major;         /* Major version: breaking changes. */
    int minor;         /* Minor version: backwards-compatible features. */
    int patch;         /* Patch version: backwards-compatible fixes. */
    VersionMode mode;  /* Additional version mode. */
} Version;

//test

/// @brief semantic version
///
/// contains something
typedef struct {
    int foo;
} SemVers;

//does something
int my_function(int x, float y);

#endif /* VERSION_H */";
        let tree = parse_sourecode(code).expect("tree can be constructed");
        assert_eq!(
            query_type_definition_linerange(code, "VersionMode".as_bytes(), tree.root_node()),
            Ok(LineRange { start: 4, end: 9 })
        );

        assert_eq!(
            query_type_definition_linerange(code, "Version".as_bytes(), tree.root_node()),
            Ok(LineRange { start: 11, end: 17 })
        );

        assert_eq!(
            query_type_definition_linerange(code, "SemVers".as_bytes(), tree.root_node()),
            Ok(LineRange { start: 21, end: 26 })
        );

        assert_eq!(
            query_function_definition_linerange(code, "my_function".as_bytes(), tree.root_node()),
            Ok(LineRange { start: 28, end: 29 })
        );
    }
    #[test]
    fn test_example2_code_yields_correct_ranges() {
        let code = b"#ifndef FORTRAN_ICD_H
#define FORTRAN_ICD_H

/* Status codes returned by ICD functions. */
typedef enum {
    ICD_OK = 0,
    ICD_ERROR = 1
} ICD_Status;

/* Input data exchanged with Fortran. */
typedef struct {
    int id;
    double value;
} ICD_Data;

/* Initializes the ICD interface. */
ICD_Status icd_initialize(void);

/* Processes input data and returns a result. */
ICD_Status icd_process(const ICD_Data *input, ICD_Data *output);

/* Releases interface resources. */
void icd_finalize(void);

#endif /* FORTRAN_ICD_H */";
        let tree = parse_sourecode(code).expect("tree can be constructed");
        assert_eq!(
            query_type_definition_linerange(code, "ICD_Status".as_bytes(), tree.root_node()),
            Ok(LineRange { start: 4, end: 8 })
        );
    }
}

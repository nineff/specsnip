///
///
/// - code ():
/// - identifier ():
/// - mode ():
/// ->
#let function-linerange(code, identifier) = {
  if type(code) == path { code = read(code) }
  cbor(plugin("specsnip.wasm").get_function_definition_line_range(bytes(code), bytes(identifier)))
}

#let typedef-linerange(code, identifier) = {
  if type(code) == path { code = read(code) }
  cbor(plugin("specsnip.wasm").get_type_definition_line_range(bytes(code), bytes(identifier)))
}

#let query_code(code, query) = {
  if type(code) == path { code = read(code) }
  cbor(plugin("specsnip.wasm").query_tree(bytes(code), bytes(query)))
}

#let get_tree(code) = {
  if type(code) == path { code = read(code) }
  cbor(plugin("specsnip.wasm").get_tree(bytes(code)))
}

#let contains_identifier(code, identifier) = {
  if type(code) == path { code = read(code) }
  cbor(plugin("specsnip.wasm").check_identifier_existance(bytes(code), bytes(identifier)))
}

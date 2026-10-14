# SpecSnip

SpecSnip is a Typst package for querying C header files for type- or function definitions, as commonly found in formal Interface Control Documents (ICDs).

Its goal is to make it easier and more reliable to print excerpts from these headers without manually searching for them or updating line numbers. Instead, it lets you search for identifiers by name.

It uses [Tree-Sitter](https://tree-sitter.github.io/tree-sitter/) compiled to WASM to perform the searches. This also enables custom queries for other source code elements.

## Example

```typst
#import "@preview/specsnip:0.1.0": *
//codly is optional
#import "@preview/codly:1.3.0": *
#import "@preview/codly-languages:0.1.10": *
#show: codly-init.with()

#let code = read(path("icd.h"))

//get the Linenumbers where the "icd_process" function is defined including its comments.
#let r = function-linerange(code, "icd_process")

//dispay just this codeblock
#codly-range(r.start, ..r)
#raw(code, block: true, lang: "h")
```

## todo

- tests
- provide wasm sourcecode on github or somewhere else

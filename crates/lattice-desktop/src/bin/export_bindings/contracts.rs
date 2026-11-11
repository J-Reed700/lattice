//! Typed contracts for the generated commands: each command's named arguments
//! and resolved value, keyed by its snake_case name, appended to `bindings.ts`.

use std::collections::BTreeMap;

const METHOD: &str = "async ";
const INVOKE: &str = "TAURI_INVOKE(\"plugin:";

struct Param<'a> {
    name: &'a str,
    optional: bool,
    ty: &'a str,
}

fn slice(text: &str, from: usize, to: usize) -> &str {
    text.get(from..to).unwrap_or_default()
}

/// Splits `text` at each `sep` outside every `<>`, `{}`, `()` and `[]`.
fn split_top(text: &str, sep: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut start = 0usize;
    for (at, c) in text.char_indices() {
        match c {
            '<' | '{' | '(' | '[' => depth += 1,
            '>' | '}' | ')' | ']' => depth -= 1,
            c if c == sep && depth == 0 => {
                parts.push(slice(text, start, at));
                start = at + c.len_utf8();
            }
            _ => {}
        }
    }
    parts.push(slice(text, start, text.len()));
    parts
}

/// The index of the `)` closing the `(` that opens `text`.
fn closing_paren(text: &str) -> Option<usize> {
    let mut depth = 0i32;
    for (at, c) in text.char_indices() {
        match c {
            '<' | '{' | '(' | '[' => depth += 1,
            '>' | '}' | ')' | ']' => {
                depth -= 1;
                if depth == 0 {
                    return (c == ')').then_some(at);
                }
            }
            _ => {}
        }
    }
    None
}

fn parse_params(params: &str) -> Result<Vec<Param<'_>>, String> {
    if params.trim().is_empty() {
        return Ok(Vec::new());
    }
    split_top(params, ',')
        .into_iter()
        .map(|param| {
            let param = param.trim();
            let (name, ty) = param
                .split_once(": ")
                .ok_or_else(|| format!("parameter `{param}` is not `name: type`"))?;
            let optional = split_top(ty, '|').iter().any(|part| part.trim() == "null");
            Ok(Param {
                name: name.trim(),
                optional,
                ty: ty.trim(),
            })
        })
        .collect()
}

fn render_args(params: &[Param<'_>]) -> String {
    if params.is_empty() {
        return "void".to_string();
    }
    let fields = params
        .iter()
        .map(|param| {
            let mark = if param.optional { "?" } else { "" };
            format!("{}{mark}: {}", param.name, param.ty)
        })
        .collect::<Vec<_>>()
        .join("; ");
    let prefix = if params.iter().all(|param| param.optional) {
        "void | "
    } else {
        ""
    };
    format!("{prefix}{{ {fields} }}")
}

/// `X` for `Result<X, Y>`; any other return type unchanged.
fn render_result(ret: &str) -> String {
    let inner = ret
        .strip_prefix("Result<")
        .and_then(|rest| rest.strip_suffix('>'));
    let Some(inner) = inner else {
        return ret.to_string();
    };
    let mut depth = 0i32;
    for c in inner.chars() {
        match c {
            '<' | '{' | '(' | '[' => depth += 1,
            '>' | '}' | ')' | ']' => depth -= 1,
            _ => {}
        }
        if depth < 0 {
            return ret.to_string();
        }
    }
    split_top(inner, ',')
        .first()
        .map_or_else(|| ret.to_string(), |ok| ok.trim().to_string())
}

/// The args and result of one method header line.
fn parse_header(line: &str) -> Result<(String, String), String> {
    let header = line.trim_end();
    if header.contains("=>") {
        return Err(format!(
            "`{header}` has an arrow type, which the contract generator cannot read"
        ));
    }
    let unreadable = || format!("`{header}` is not `async name(params) : Promise<ret> {{`");
    let rest = header.strip_prefix(METHOD).ok_or_else(unreadable)?;
    let open = rest.find('(').ok_or_else(unreadable)?;
    let (_, from_open) = rest.split_at(open);
    let close = closing_paren(from_open).ok_or_else(unreadable)?;
    let (params_with_open, after) = from_open.split_at(close);
    let params = params_with_open.get(1..).ok_or_else(unreadable)?;
    let ret = after
        .strip_prefix(") : Promise<")
        .and_then(|rest| rest.strip_suffix("> {"))
        .ok_or_else(unreadable)?;
    Ok((render_args(&parse_params(params)?), render_result(ret)))
}

/// The snake_case command named by the first route invoked in `region`.
fn invoked_command(region: &str) -> Option<&str> {
    let (_, after) = region.split_once(INVOKE)?;
    let (route, _) = after.split_once('"')?;
    route.split_once('|').map(|(_, command)| command)
}

/// The `CommandArgs` and `CommandResults` maps for the qualified bindings.
pub fn render_contracts(bindings: &str) -> Result<String, String> {
    let mut headers: Vec<(usize, &str)> = Vec::new();
    let mut offset = 0usize;
    for line in bindings.split_inclusive('\n') {
        if line.starts_with(METHOD) {
            headers.push((offset, line));
        }
        offset += line.len();
    }
    let mut entries: BTreeMap<&str, (String, String)> = BTreeMap::new();
    for (index, (start, line)) in headers.iter().enumerate() {
        let end = headers.get(index + 1).map_or(bindings.len(), |next| next.0);
        let region = slice(bindings, start + line.len(), end);
        let command = invoked_command(region).ok_or_else(|| {
            format!(
                "`{}` has no TAURI_INVOKE route before the next method",
                line.trim_end()
            )
        })?;
        let contract = parse_header(line)?;
        if entries.insert(command, contract).is_some() {
            return Err(format!("command `{command}` is generated more than once"));
        }
    }
    let mut args = String::new();
    let mut results = String::new();
    for (command, (command_args, command_result)) in &entries {
        args.push_str(&format!("\t{command}: {command_args};\n"));
        results.push_str(&format!("\t{command}: {command_result};\n"));
    }
    Ok(format!(
        "/** Each command's named arguments, keyed by command. `void` when none is required. */\nexport type CommandArgs = {{\n{args}}};\n\n/** What each command resolves with, keyed by command. */\nexport type CommandResults = {{\n{results}}};\n"
    ))
}

#[cfg(test)]
mod tests {
    use super::render_contracts;

    fn method(camel: &str, params: &str, ret: &str, snake: &str) -> String {
        format!(
            "async {camel}({params}) : Promise<{ret}> {{\n    try {{\n    return {{ status: \"ok\", data: await TAURI_INVOKE(\"plugin:demo|{snake}\", {{ }}) }};\n}} catch (e) {{\n}}\n}},\n"
        )
    }

    fn render(methods: &[String]) -> Result<String, String> {
        render_contracts(&format!(
            "export const commands = {{\n{}}}\n",
            methods.concat()
        ))
    }

    fn lines_between<'a>(block: &'a str, start: &str) -> Vec<&'a str> {
        block
            .lines()
            .skip_while(|line| !line.starts_with(start))
            .skip(1)
            .take_while(|line| *line != "};")
            .collect()
    }

    #[test]
    fn params_become_named_args_and_a_nullable_param_is_optional() -> Result<(), String> {
        let block = render(&[method(
            "generateProgram",
            "request: RequestDto, requestId: string | null, onProgress: TAURI_CHANNEL<ProgressDto>",
            "Result<ProgramDto, ApiError>",
            "generate_program",
        )])?;
        let args = lines_between(&block, "export type CommandArgs");
        assert_eq!(
            args,
            vec!["\tgenerate_program: { request: RequestDto; requestId?: string | null; onProgress: TAURI_CHANNEL<ProgressDto> };"]
        );
        Ok(())
    }

    #[test]
    fn a_command_without_params_takes_void() -> Result<(), String> {
        let block = render(&[method("rescan", "", "Result<null, ApiError>", "rescan")])?;
        assert_eq!(
            lines_between(&block, "export type CommandArgs"),
            vec!["\trescan: void;"]
        );
        Ok(())
    }

    #[test]
    fn a_command_whose_params_are_all_optional_may_be_called_without_args() -> Result<(), String> {
        let block = render(&[method(
            "listThings",
            "query: string | null, limit: number | null",
            "Result<string[], ApiError>",
            "list_things",
        )])?;
        assert_eq!(
            lines_between(&block, "export type CommandArgs"),
            vec!["\tlist_things: void | { query?: string | null; limit?: number | null };"]
        );
        Ok(())
    }

    #[test]
    fn a_result_return_gives_its_ok_type_and_a_plain_return_is_kept() -> Result<(), String> {
        let block = render(&[
            method("a", "", "Result<ListingDto, ApiError>", "a"),
            method("b", "", "LearningRuntimePresetDto[]", "b"),
            method(
                "c",
                "",
                "Result<{ [key in string]: JsonValue }, ApiError>",
                "c",
            ),
        ])?;
        assert_eq!(
            lines_between(&block, "export type CommandResults"),
            vec![
                "\ta: ListingDto;",
                "\tb: LearningRuntimePresetDto[];",
                "\tc: { [key in string]: JsonValue };",
            ]
        );
        Ok(())
    }

    #[test]
    fn commas_inside_brackets_do_not_split_a_param() -> Result<(), String> {
        let block = render(&[method(
            "m",
            "a: { [key in string]: JsonValue }, b: Partial<{ a: string, b: number }>, c: TAURI_CHANNEL<Foo>",
            "Result<Map<string, number>, ApiError>",
            "m",
        )])?;
        assert_eq!(
            lines_between(&block, "export type CommandArgs"),
            vec!["\tm: { a: { [key in string]: JsonValue }; b: Partial<{ a: string, b: number }>; c: TAURI_CHANNEL<Foo> };"]
        );
        assert_eq!(
            lines_between(&block, "export type CommandResults"),
            vec!["\tm: Map<string, number>;"]
        );
        Ok(())
    }

    #[test]
    fn entries_are_sorted_by_snake_name() -> Result<(), String> {
        let block = render(&[
            method("zeta", "", "Result<null, ApiError>", "zeta"),
            method("alpha", "", "Result<null, ApiError>", "alpha"),
            method("mid", "", "Result<null, ApiError>", "mid"),
        ])?;
        assert_eq!(
            lines_between(&block, "export type CommandArgs"),
            vec!["\talpha: void;", "\tmid: void;", "\tzeta: void;"]
        );
        assert!(block.starts_with("/** Each command's named arguments, keyed by command. `void` when none is required. */\nexport type CommandArgs = {\n"));
        assert!(block.contains("};\n\n/** What each command resolves with, keyed by command. */\nexport type CommandResults = {\n"));
        assert!(block.ends_with("};\n"));
        Ok(())
    }

    #[test]
    fn a_header_without_an_invoke_is_an_error() {
        let header = "async lonely() : Promise<Result<null, ApiError>> {\n}\n".to_string();
        assert!(render_contracts(&header).is_err());
        let two = format!(
            "{}{header}",
            method("first", "", "Result<null, ApiError>", "first")
        );
        assert!(render_contracts(&two).is_err());
    }

    #[test]
    fn a_duplicate_snake_name_is_an_error() {
        let result = render(&[
            method("one", "", "Result<null, ApiError>", "same"),
            method("two", "", "Result<null, ApiError>", "same"),
        ]);
        assert!(result.is_err());
    }

    #[test]
    fn a_header_with_an_arrow_type_is_an_error() {
        let result = render(&[method(
            "m",
            "callback: (x: number) => void",
            "Result<null, ApiError>",
            "m",
        )]);
        assert!(result.is_err());
    }
}

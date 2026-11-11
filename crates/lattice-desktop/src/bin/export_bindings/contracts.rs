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

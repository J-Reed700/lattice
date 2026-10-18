-- A chat's own model choice. NULL means "the Settings default, live".
-- Otherwise a ModelRef as JSON: {"kind":"local","model_id":"..."} etc.
-- The Learning role needs no column: it is `llm.learning_model` in Settings.
ALTER TABLE conversations ADD COLUMN model_choice TEXT;

-- The old column recorded the frontend's guess at creation and was never
-- read by a turn. Provenance now lives on each assistant message.
ALTER TABLE conversations DROP COLUMN model_name;

-- The per-space default model was a third tier nothing resolved from.
-- Resolution is: request override > chat choice > Settings default.
ALTER TABLE conversation_spaces DROP COLUMN default_model_name;
ALTER TABLE journals DROP COLUMN default_model_name;

// @generated automatically by Diesel CLI.

diesel::table! {
    logs (id) {
        id -> Integer,
        log_date -> Text,
        energy -> Integer,
        mvos -> Text,
        worked -> Text,
        failed -> Text,
        output -> Text,
    }
}

diesel::table! {
    tasks (id) {
        id -> Integer,
        log_id -> Integer,
        priority -> Nullable<Text>,
        completion_date -> Nullable<Text>,
        creation_date -> Nullable<Text>,
        project_tag -> Nullable<Text>,
        context_tag -> Nullable<Text>,
        key_value_tags -> Text,
        raw_line -> Text,
        is_completed -> Bool,
    }
}

diesel::table! {
    todo_cache (raw_line) {
        raw_line -> Text,
    }
}

diesel::joinable!(tasks -> logs (log_id));

diesel::allow_tables_to_appear_in_same_query!(
    logs,
    tasks,
    todo_cache,
);

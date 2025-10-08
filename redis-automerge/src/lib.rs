pub mod ext;

use std::os::raw::{c_int, c_void};

use automerge::Change;
use ext::{RedisAutomergeClient, RedisAutomergeExt};
#[cfg(not(test))]
use redis_module::redis_module;
use redis_module::{
    native_types::RedisType,
    raw::{self, Status},
    Context, NextArg, RedisError, RedisResult, RedisString, RedisValue,
};

static REDIS_AUTOMERGE_TYPE: RedisType = RedisType::new(
    "amdoc-rs1",
    0,
    raw::RedisModuleTypeMethods {
        version: raw::REDISMODULE_TYPE_METHOD_VERSION as u64,
        rdb_load: Some(am_rdb_load),
        rdb_save: Some(am_rdb_save),
        aof_rewrite: None,
        free: Some(am_free),
        mem_usage: None,
        digest: None,
        aux_load: None,
        aux_save: None,
        aux_save2: None,
        aux_save_triggers: 0,
        free_effort: None,
        unlink: None,
        copy: None,
        defrag: None,
        copy2: None,
        free_effort2: None,
        mem_usage2: None,
        unlink2: None,
    },
);

fn init(ctx: &Context, _args: &Vec<RedisString>) -> Status {
    REDIS_AUTOMERGE_TYPE
        .create_data_type(ctx.ctx)
        .map(|_| Status::Ok)
        .unwrap_or(Status::Err)
}

/// Helper function to parse a RedisString as UTF-8 with a custom error message.
fn parse_utf8_field<'a>(s: &'a RedisString, field_name: &str) -> Result<&'a str, RedisError> {
    s.try_as_str()
        .map_err(|_| RedisError::String(format!("{} must be utf-8", field_name)))
}

/// Helper function to parse a RedisString as UTF-8 (generic "value" error).
fn parse_utf8_value(s: &RedisString) -> Result<&str, RedisError> {
    s.try_as_str()
        .map_err(|_| RedisError::Str("value must be utf-8"))
}

fn am_load(ctx: &Context, args: Vec<RedisString>) -> RedisResult {
    let mut args = args.into_iter().skip(1);
    let key_name = args.next_arg()?;
    let data = args.next_arg()?;
    let client = RedisAutomergeClient::load(data.as_slice())
        .map_err(|e| RedisError::String(e.to_string()))?;
    let key = ctx.open_key_writable(&key_name);
    key.set_value(&REDIS_AUTOMERGE_TYPE, client)?;
    Ok(RedisValue::SimpleStringStatic("OK"))
}

fn am_new(ctx: &Context, args: Vec<RedisString>) -> RedisResult {
    if args.len() != 2 {
        return Err(RedisError::WrongArity);
    }
    let key_name = &args[1];
    let key = ctx.open_key_writable(key_name);
    key.set_value(&REDIS_AUTOMERGE_TYPE, RedisAutomergeClient::new())?;
    ctx.replicate("am.new", &[key_name]);
    Ok(RedisValue::SimpleStringStatic("OK"))
}

fn am_save(ctx: &Context, args: Vec<RedisString>) -> RedisResult {
    let mut args = args.into_iter().skip(1);
    let key_name = args.next_arg()?;
    let key = ctx.open_key(&key_name);
    let client = key
        .get_value::<RedisAutomergeClient>(&REDIS_AUTOMERGE_TYPE)?
        .ok_or(RedisError::Str("no such key"))?;
    Ok(RedisValue::StringBuffer(client.save()))
}

fn am_puttext(ctx: &Context, args: Vec<RedisString>) -> RedisResult {
    if args.len() != 4 {
        return Err(RedisError::WrongArity);
    }
    let key_name = &args[1];
    let field = parse_utf8_field(&args[2], "field")?;
    let value = parse_utf8_value(&args[3])?;
    let key = ctx.open_key_writable(key_name);
    let client = key
        .get_value::<RedisAutomergeClient>(&REDIS_AUTOMERGE_TYPE)?
        .ok_or(RedisError::Str("no such key"))?;
    client
        .put_text(field, value)
        .map_err(|e| RedisError::String(e.to_string()))?;
    let refs: Vec<&RedisString> = args[1..].iter().collect();
    ctx.replicate("am.puttext", &refs[..]);
    Ok(RedisValue::SimpleStringStatic("OK"))
}

fn am_gettext(ctx: &Context, args: Vec<RedisString>) -> RedisResult {
    if args.len() != 3 {
        return Err(RedisError::WrongArity);
    }
    let key_name = &args[1];
    let field = parse_utf8_field(&args[2], "field")?;
    let key = ctx.open_key(key_name);
    let client = key
        .get_value::<RedisAutomergeClient>(&REDIS_AUTOMERGE_TYPE)?
        .ok_or(RedisError::Str("no such key"))?;
    match client
        .get_text(field)
        .map_err(|e| RedisError::String(e.to_string()))?
    {
        Some(text) => Ok(RedisValue::BulkString(text)),
        None => Ok(RedisValue::Null),
    }
}

fn am_putint(ctx: &Context, args: Vec<RedisString>) -> RedisResult {
    if args.len() != 4 {
        return Err(RedisError::WrongArity);
    }
    let key_name = &args[1];
    let field = parse_utf8_field(&args[2], "field")?;
    let value: i64 = args[3]
        .parse_integer()
        .map_err(|_| RedisError::Str("value must be an integer"))?;
    let key = ctx.open_key_writable(key_name);
    let client = key
        .get_value::<RedisAutomergeClient>(&REDIS_AUTOMERGE_TYPE)?
        .ok_or(RedisError::Str("no such key"))?;
    client
        .put_int(field, value)
        .map_err(|e| RedisError::String(e.to_string()))?;
    let refs: Vec<&RedisString> = args[1..].iter().collect();
    ctx.replicate("am.putint", &refs[..]);
    Ok(RedisValue::SimpleStringStatic("OK"))
}

fn am_getint(ctx: &Context, args: Vec<RedisString>) -> RedisResult {
    if args.len() != 3 {
        return Err(RedisError::WrongArity);
    }
    let key_name = &args[1];
    let field = parse_utf8_field(&args[2], "field")?;
    let key = ctx.open_key(key_name);
    let client = key
        .get_value::<RedisAutomergeClient>(&REDIS_AUTOMERGE_TYPE)?
        .ok_or(RedisError::Str("no such key"))?;
    match client
        .get_int(field)
        .map_err(|e| RedisError::String(e.to_string()))?
    {
        Some(value) => Ok(RedisValue::Integer(value)),
        None => Ok(RedisValue::Null),
    }
}

fn am_putdouble(ctx: &Context, args: Vec<RedisString>) -> RedisResult {
    if args.len() != 4 {
        return Err(RedisError::WrongArity);
    }
    let key_name = &args[1];
    let field = parse_utf8_field(&args[2], "field")?;
    let value: f64 = parse_utf8_value(&args[3])?
        .parse()
        .map_err(|_| RedisError::Str("value must be a valid double"))?;
    let key = ctx.open_key_writable(key_name);
    let client = key
        .get_value::<RedisAutomergeClient>(&REDIS_AUTOMERGE_TYPE)?
        .ok_or(RedisError::Str("no such key"))?;
    client
        .put_double(field, value)
        .map_err(|e| RedisError::String(e.to_string()))?;
    let refs: Vec<&RedisString> = args[1..].iter().collect();
    ctx.replicate("am.putdouble", &refs[..]);
    Ok(RedisValue::SimpleStringStatic("OK"))
}

fn am_getdouble(ctx: &Context, args: Vec<RedisString>) -> RedisResult {
    if args.len() != 3 {
        return Err(RedisError::WrongArity);
    }
    let key_name = &args[1];
    let field = parse_utf8_field(&args[2], "field")?;
    let key = ctx.open_key(key_name);
    let client = key
        .get_value::<RedisAutomergeClient>(&REDIS_AUTOMERGE_TYPE)?
        .ok_or(RedisError::Str("no such key"))?;
    match client
        .get_double(field)
        .map_err(|e| RedisError::String(e.to_string()))?
    {
        Some(value) => Ok(RedisValue::Float(value)),
        None => Ok(RedisValue::Null),
    }
}

fn am_putbool(ctx: &Context, args: Vec<RedisString>) -> RedisResult {
    if args.len() != 4 {
        return Err(RedisError::WrongArity);
    }
    let key_name = &args[1];
    let field = parse_utf8_field(&args[2], "field")?;
    let value_str = parse_utf8_value(&args[3])?;
    let value = match value_str.to_lowercase().as_str() {
        "true" | "1" => true,
        "false" | "0" => false,
        _ => return Err(RedisError::Str("value must be true/false or 1/0")),
    };
    let key = ctx.open_key_writable(key_name);
    let client = key
        .get_value::<RedisAutomergeClient>(&REDIS_AUTOMERGE_TYPE)?
        .ok_or(RedisError::Str("no such key"))?;
    client
        .put_bool(field, value)
        .map_err(|e| RedisError::String(e.to_string()))?;
    let refs: Vec<&RedisString> = args[1..].iter().collect();
    ctx.replicate("am.putbool", &refs[..]);
    Ok(RedisValue::SimpleStringStatic("OK"))
}

fn am_getbool(ctx: &Context, args: Vec<RedisString>) -> RedisResult {
    if args.len() != 3 {
        return Err(RedisError::WrongArity);
    }
    let key_name = &args[1];
    let field = parse_utf8_field(&args[2], "field")?;
    let key = ctx.open_key(key_name);
    let client = key
        .get_value::<RedisAutomergeClient>(&REDIS_AUTOMERGE_TYPE)?
        .ok_or(RedisError::Str("no such key"))?;
    match client
        .get_bool(field)
        .map_err(|e| RedisError::String(e.to_string()))?
    {
        Some(value) => Ok(RedisValue::Integer(if value { 1 } else { 0 })),
        None => Ok(RedisValue::Null),
    }
}

fn am_apply(ctx: &Context, args: Vec<RedisString>) -> RedisResult {
    if args.len() < 3 {
        return Err(RedisError::WrongArity);
    }
    let key_name = &args[1];
    let key = ctx.open_key_writable(key_name);
    let client = key
        .get_value::<RedisAutomergeClient>(&REDIS_AUTOMERGE_TYPE)?
        .ok_or(RedisError::Str("no such key"))?;
    let mut changes = Vec::new();
    for change_str in &args[2..] {
        let bytes = change_str.to_vec();
        let change = Change::from_bytes(bytes)
            .map_err(|e| RedisError::String(format!("invalid change: {}", e)))?;
        changes.push(change);
    }
    client
        .apply(changes)
        .map_err(|e| RedisError::String(e.to_string()))?;
    let refs: Vec<&RedisString> = args[1..].iter().collect();
    ctx.replicate("am.apply", &refs[..]);
    Ok(RedisValue::SimpleStringStatic("OK"))
}

/// # Safety
/// This function is called by Redis when freeing a RedisAutomergeClient value.
/// The caller (Redis) must ensure that `value` is a valid pointer to a
/// RedisAutomergeClient that was previously allocated via Box::into_raw.
unsafe extern "C" fn am_free(value: *mut c_void) {
    drop(Box::from_raw(value.cast::<RedisAutomergeClient>()));
}

/// # Safety
/// This function is called by Redis during RDB persistence.
/// The caller (Redis) must ensure that `rdb` is a valid RedisModuleIO pointer
/// and `value` is a valid pointer to a RedisAutomergeClient.
unsafe extern "C" fn am_rdb_save(rdb: *mut raw::RedisModuleIO, value: *mut c_void) {
    let client = &*(value.cast::<RedisAutomergeClient>());
    raw::save_slice(rdb, &client.save());
}

/// # Safety
/// This function is called by Redis during RDB loading.
/// The caller (Redis) must ensure that `rdb` is a valid RedisModuleIO pointer.
/// Returns a pointer to a newly allocated RedisAutomergeClient, or null on error.
unsafe extern "C" fn am_rdb_load(rdb: *mut raw::RedisModuleIO, _encver: c_int) -> *mut c_void {
    match raw::load_string_buffer(rdb) {
        Ok(buf) => match RedisAutomergeClient::load(buf.as_ref()) {
            Ok(client) => Box::into_raw(Box::new(client)).cast::<c_void>(),
            Err(_) => std::ptr::null_mut(),
        },
        Err(_) => std::ptr::null_mut(),
    }
}

#[cfg(not(test))]
redis_module! {
    name: "automerge",
    version: 1,
    allocator: (redis_module::alloc::RedisAlloc, redis_module::alloc::RedisAlloc),
    data_types: [REDIS_AUTOMERGE_TYPE],
    init: init,
    commands: [
        ["am.new", am_new, "write", 1, 1, 1],
        ["am.load", am_load, "write", 1, 1, 1],
        ["am.save", am_save, "readonly", 1, 1, 1],
        ["am.apply", am_apply, "write", 1, 1, 1],
        ["am.puttext", am_puttext, "write", 1, 1, 1],
        ["am.gettext", am_gettext, "readonly", 1, 1, 1],
        ["am.putint", am_putint, "write", 1, 1, 1],
        ["am.getint", am_getint, "readonly", 1, 1, 1],
        ["am.putdouble", am_putdouble, "write", 1, 1, 1],
        ["am.getdouble", am_getdouble, "readonly", 1, 1, 1],
        ["am.putbool", am_putbool, "write", 1, 1, 1],
        ["am.getbool", am_getbool, "readonly", 1, 1, 1],
    ],
}

#[cfg(test)]
mod tests {
    use super::*;
    use automerge::{transaction::Transactable, Automerge, ReadDoc, ROOT};

    #[test]
    fn apply_and_persist() {
        // Build a change on a separate document.
        let mut base = Automerge::new();
        let mut tx = base.transaction();
        tx.put(ROOT, "field", 1).unwrap();
        let (hash, _) = tx.commit();
        let change = base.get_change_by_hash(&hash.unwrap()).unwrap();

        // Apply the change using the client.
        let mut client = RedisAutomergeClient::new();
        client.apply(vec![change.clone()]).unwrap();

        // AOF should capture the change.
        let aof = client.commands();
        assert_eq!(aof.len(), 1);

        // RDB persistence roundtrip.
        let bytes = client.save();
        let loaded = RedisAutomergeClient::load(&bytes).unwrap();
        assert_eq!(loaded.save(), bytes);
    }

    #[test]
    fn put_and_get_text_roundtrip() {
        let mut client = RedisAutomergeClient::new();
        client.put_text("greeting", "hello").unwrap();
        assert_eq!(
            client.get_text("greeting").unwrap(),
            Some("hello".to_string())
        );

        let bytes = client.save();
        let loaded = RedisAutomergeClient::load(&bytes).unwrap();
        assert_eq!(
            loaded.get_text("greeting").unwrap(),
            Some("hello".to_string())
        );
    }

    #[test]
    fn put_and_get_int_roundtrip() {
        let mut client = RedisAutomergeClient::new();
        client.put_int("age", 42).unwrap();
        assert_eq!(client.get_int("age").unwrap(), Some(42));

        let bytes = client.save();
        let loaded = RedisAutomergeClient::load(&bytes).unwrap();
        assert_eq!(loaded.get_int("age").unwrap(), Some(42));
    }

    #[test]
    fn put_and_get_int_negative() {
        let mut client = RedisAutomergeClient::new();
        client.put_int("temperature", -10).unwrap();
        assert_eq!(client.get_int("temperature").unwrap(), Some(-10));
    }

    #[test]
    fn put_and_get_double_roundtrip() {
        let mut client = RedisAutomergeClient::new();
        client.put_double("pi", 3.14159).unwrap();
        assert_eq!(client.get_double("pi").unwrap(), Some(3.14159));

        let bytes = client.save();
        let loaded = RedisAutomergeClient::load(&bytes).unwrap();
        assert_eq!(loaded.get_double("pi").unwrap(), Some(3.14159));
    }

    #[test]
    fn put_and_get_bool_roundtrip() {
        let mut client = RedisAutomergeClient::new();
        client.put_bool("active", true).unwrap();
        assert_eq!(client.get_bool("active").unwrap(), Some(true));

        client.put_bool("disabled", false).unwrap();
        assert_eq!(client.get_bool("disabled").unwrap(), Some(false));

        let bytes = client.save();
        let loaded = RedisAutomergeClient::load(&bytes).unwrap();
        assert_eq!(loaded.get_bool("active").unwrap(), Some(true));
        assert_eq!(loaded.get_bool("disabled").unwrap(), Some(false));
    }

    #[test]
    fn get_nonexistent_fields() {
        let client = RedisAutomergeClient::new();
        assert_eq!(client.get_text("missing").unwrap(), None);
        assert_eq!(client.get_int("missing").unwrap(), None);
        assert_eq!(client.get_double("missing").unwrap(), None);
        assert_eq!(client.get_bool("missing").unwrap(), None);
    }

    #[test]
    fn mixed_types_in_document() {
        let mut client = RedisAutomergeClient::new();
        client.put_text("name", "Alice").unwrap();
        client.put_int("age", 30).unwrap();
        client.put_double("height", 5.6).unwrap();
        client.put_bool("verified", true).unwrap();

        assert_eq!(client.get_text("name").unwrap(), Some("Alice".to_string()));
        assert_eq!(client.get_int("age").unwrap(), Some(30));
        assert_eq!(client.get_double("height").unwrap(), Some(5.6));
        assert_eq!(client.get_bool("verified").unwrap(), Some(true));

        let bytes = client.save();
        let loaded = RedisAutomergeClient::load(&bytes).unwrap();
        assert_eq!(loaded.get_text("name").unwrap(), Some("Alice".to_string()));
        assert_eq!(loaded.get_int("age").unwrap(), Some(30));
        assert_eq!(loaded.get_double("height").unwrap(), Some(5.6));
        assert_eq!(loaded.get_bool("verified").unwrap(), Some(true));
    }

    #[test]
    fn nested_path_operations() {
        let mut client = RedisAutomergeClient::new();

        // Test nested text field
        client.put_text("user.profile.name", "Bob").unwrap();
        assert_eq!(
            client.get_text("user.profile.name").unwrap(),
            Some("Bob".to_string())
        );

        // Test nested int field
        client.put_int("user.profile.age", 25).unwrap();
        assert_eq!(client.get_int("user.profile.age").unwrap(), Some(25));

        // Test nested double field
        client.put_double("metrics.cpu.usage", 75.5).unwrap();
        assert_eq!(client.get_double("metrics.cpu.usage").unwrap(), Some(75.5));

        // Test nested bool field
        client.put_bool("flags.features.enabled", true).unwrap();
        assert_eq!(
            client.get_bool("flags.features.enabled").unwrap(),
            Some(true)
        );

        // Test that nonexistent nested paths return None
        assert_eq!(client.get_text("user.profile.email").unwrap(), None);
        assert_eq!(client.get_int("missing.path.value").unwrap(), None);
    }

    #[test]
    fn nested_path_with_dollar_prefix() {
        let mut client = RedisAutomergeClient::new();

        // Test with $ prefix (JSONPath style)
        client.put_text("$.user.name", "Charlie").unwrap();
        assert_eq!(
            client.get_text("$.user.name").unwrap(),
            Some("Charlie".to_string())
        );

        // Verify that the same path without $ works
        assert_eq!(
            client.get_text("user.name").unwrap(),
            Some("Charlie".to_string())
        );
    }

    #[test]
    fn nested_path_persistence() {
        let mut client = RedisAutomergeClient::new();

        // Create nested structure
        client.put_text("user.profile.name", "Diana").unwrap();
        client.put_int("user.profile.age", 28).unwrap();
        client.put_double("user.metrics.score", 95.7).unwrap();
        client.put_bool("user.active", true).unwrap();

        // Persist and reload
        let bytes = client.save();
        let loaded = RedisAutomergeClient::load(&bytes).unwrap();

        // Verify all nested values are preserved
        assert_eq!(
            loaded.get_text("user.profile.name").unwrap(),
            Some("Diana".to_string())
        );
        assert_eq!(loaded.get_int("user.profile.age").unwrap(), Some(28));
        assert_eq!(loaded.get_double("user.metrics.score").unwrap(), Some(95.7));
        assert_eq!(loaded.get_bool("user.active").unwrap(), Some(true));
    }

    #[test]
    fn deeply_nested_paths() {
        let mut client = RedisAutomergeClient::new();

        // Test deeply nested path
        client
            .put_text("a.b.c.d.e.f.value", "deeply nested")
            .unwrap();
        assert_eq!(
            client.get_text("a.b.c.d.e.f.value").unwrap(),
            Some("deeply nested".to_string())
        );

        // Verify persistence
        let bytes = client.save();
        let loaded = RedisAutomergeClient::load(&bytes).unwrap();
        assert_eq!(
            loaded.get_text("a.b.c.d.e.f.value").unwrap(),
            Some("deeply nested".to_string())
        );
    }

    #[test]
    fn mixed_nested_and_flat_keys() {
        let mut client = RedisAutomergeClient::new();

        // Mix flat and nested keys
        client.put_text("simple", "flat value").unwrap();
        client.put_text("nested.key", "nested value").unwrap();

        assert_eq!(
            client.get_text("simple").unwrap(),
            Some("flat value".to_string())
        );
        assert_eq!(
            client.get_text("nested.key").unwrap(),
            Some("nested value".to_string())
        );
    }
}

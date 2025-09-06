pub mod ext;

use std::os::raw::{c_int, c_void};

use automerge::Change;
use ext::{RedisAutomergeClient, RedisAutomergeExt};
use redis_module::{
    native_types::RedisType,
    raw::{self, Status},
    Context,
    NextArg,
    RedisError,
    RedisResult,
    RedisString,
    RedisValue,
};
#[cfg(not(test))]
use redis_module::redis_module;

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
    let field = args[2]
        .try_as_str()
        .map_err(|_| RedisError::Str("field must be utf-8"))?;
    let value = args[3]
        .try_as_str()
        .map_err(|_| RedisError::Str("value must be utf-8"))?;
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
    let field = args[2]
        .try_as_str()
        .map_err(|_| RedisError::Str("field must be utf-8"))?;
    let key = ctx.open_key(key_name);
    let client = key
        .get_value::<RedisAutomergeClient>(&REDIS_AUTOMERGE_TYPE)?
        .ok_or(RedisError::Str("no such key"))?;
    match client.get_text(field).map_err(|e| RedisError::String(e.to_string()))? {
        Some(text) => Ok(RedisValue::BulkString(text)),
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
        let change = Change::from_bytes(bytes).map_err(|_| RedisError::Str("invalid change"))?;
        changes.push(change);
    }
    client
        .apply(changes)
        .map_err(|e| RedisError::String(e.to_string()))?;
    let refs: Vec<&RedisString> = args[1..].iter().collect();
    ctx.replicate("am.apply", &refs[..]);
    Ok(RedisValue::SimpleStringStatic("OK"))
}

unsafe extern "C" fn am_free(value: *mut c_void) {
    drop(Box::from_raw(value.cast::<RedisAutomergeClient>()));
}

unsafe extern "C" fn am_rdb_save(rdb: *mut raw::RedisModuleIO, value: *mut c_void) {
    let client = &*(value.cast::<RedisAutomergeClient>());
    raw::save_slice(rdb, &client.save());
}

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
        assert_eq!(client.get_text("greeting").unwrap(), Some("hello".to_string()));

        let bytes = client.save();
        let loaded = RedisAutomergeClient::load(&bytes).unwrap();
        assert_eq!(loaded.get_text("greeting").unwrap(), Some("hello".to_string()));
    }
}

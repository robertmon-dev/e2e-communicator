use std::sync::LazyLock;

pub(super) static REGISTER_DEVICE: LazyLock<redis::Script> = LazyLock::new(|| {
    redis::Script::new(
        r#"
local stored_user_key = redis.call('GET', KEYS[3])
if stored_user_key and stored_user_key ~= ARGV[5] then return -4 end
if redis.call('SISMEMBER', KEYS[1], ARGV[1]) == 1 then
  local s = redis.call('HGET', KEYS[2], 'signing_key')
  local a = redis.call('HGET', KEYS[2], 'agreement_key')
  if s == ARGV[2] and a == ARGV[3] then return 0 end
  if not s or not a then return -2 end
  return -3
end
if redis.call('SCARD', KEYS[1]) == 0 then
  redis.call('SET', KEYS[3], ARGV[5])
  redis.call('HSET', KEYS[2], 'signing_key', ARGV[2], 'agreement_key', ARGV[3], 'registered_at', ARGV[4], 'cert', ARGV[6])
  redis.call('SADD', KEYS[1], ARGV[1])
  return 1
end
return -1
"#,
    )
});

pub(super) static LINK_DEVICE: LazyLock<redis::Script> = LazyLock::new(|| {
    redis::Script::new(
        r#"
local uid = redis.call('GET', KEYS[1])
if not uid then return -3 end
if uid ~= ARGV[5] then return -3 end
local stored_user_key = redis.call('GET', KEYS[4])
if stored_user_key and stored_user_key ~= ARGV[6] then return -4 end
local s = redis.call('HGET', KEYS[3], 'signing_key')
local a = redis.call('HGET', KEYS[3], 'agreement_key')
if s and a then
  if s == ARGV[2] and a == ARGV[3] then
    redis.call('DEL', KEYS[1])
    return 0
  end
  return -1
end
if s or a then return -2 end
redis.call('DEL', KEYS[1])
redis.call('SET', KEYS[4], ARGV[6])
redis.call('HSET', KEYS[3], 'signing_key', ARGV[2], 'agreement_key', ARGV[3], 'registered_at', ARGV[4], 'cert', ARGV[7])
redis.call('SADD', KEYS[2], ARGV[1])
return 1
"#,
    )
});

pub(super) static REVOKE_DEVICE: LazyLock<redis::Script> = LazyLock::new(|| {
    redis::Script::new(
        r#"
local tokens = redis.call('SMEMBERS', KEYS[1])
for _, t in ipairs(tokens) do
  redis.call('DEL', ARGV[2] .. t)
end
redis.call('DEL', KEYS[1])
redis.call('DEL', KEYS[2])
redis.call('SREM', KEYS[3], ARGV[1])
return #tokens
"#,
    )
});

pub(super) static CREATE_SESSION: LazyLock<redis::Script> = LazyLock::new(|| {
    redis::Script::new(
        r#"
if redis.call('HEXISTS', KEYS[1], 'signing_key') == 0 then return 0 end
redis.call('HSET', KEYS[2], 'user_id', ARGV[1], 'device_id', ARGV[2], 'expires_at', ARGV[3])
redis.call('EXPIRE', KEYS[2], ARGV[4])
redis.call('SADD', KEYS[3], ARGV[5])
redis.call('EXPIRE', KEYS[3], ARGV[4])
return 1
"#,
    )
});

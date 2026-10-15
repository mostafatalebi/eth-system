pub const USER_INSERT_QUERY: &str = "insert into users (username, password_hash, roles) values($1, $2, $3)
    returning id, username, password_hash, roles, is_active, created_at, updated_at";

pub const USER_BY_USERNAME_QUERY: &str = "select id, username, password_hash, roles, is_active, created_at, updated_at from users where username = $1";

pub const USER_COUNT_QUERY: &str = "select count(*) as count from users";

pub const USER_BY_ID_QUERY: &str = "select id, username, password_hash, roles, is_active, created_at, updated_at from users where id = $1";

pub const REFRESH_TOKEN_DELETE_EXPIRED_QUERY: &str = "delete from refresh_tokens where user_id = $1 and expires_at <= now()";

pub const REFRESH_TOKEN_INSERT_QUERY: &str = "insert into refresh_tokens (token_hash, user_id, expires_at)
    values($1, $2, now() + $3::bigint * interval '1 second')";

pub const REFRESH_TOKEN_CONSUME_QUERY: &str = "delete from refresh_tokens where token_hash = $1 and expires_at > now() returning user_id";

pub const USER_DISABLE_QUERY: &str = "with disabled as (update users set is_active = false, updated_at = now() where username = $1 returning id),
    revoked as (delete from refresh_tokens where user_id in (select id from disabled))
    select id from disabled";

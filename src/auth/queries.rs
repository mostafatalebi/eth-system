pub const USER_INSERT_QUERY: &str = "insert into users (username, password_hash, roles) values($1, $2, $3)
    returning id, username, password_hash, roles, is_active, created_at, updated_at";

pub const USER_BY_USERNAME_QUERY: &str = "select id, username, password_hash, roles, is_active, created_at, updated_at from users where username = $1";

pub const USER_BY_ID_QUERY: &str = "select id, username, password_hash, roles, is_active, created_at, updated_at from users where id = $1";

pub const USER_DISABLE_QUERY: &str = "update users set is_active = false, updated_at = now() where username = $1 returning id";

-- Add up migration script here
CREATE TABLE sessions (
	id BIGSERIAL PRIMARY KEY,
	token VARCHAR(255) UNIQUE NOT NULL,
	user_id BIGINT NOT NULL,
	FOREIGN KEY (user_id) REFERENCES users(id)
);

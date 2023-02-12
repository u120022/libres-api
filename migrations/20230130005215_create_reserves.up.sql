-- Add up migration script here
CREATE TABLE reserves (
	id BIGSERIAL PRIMARY KEY,
	user_id BIGINT NOT NULL,
	library_name VARCHAR(255) NOT NULL,
	isbn VARCHAR(255) NOT NULL,
	state VARCHAR(255) NOT NULL,
	staging_at Timestamp NOT NULL,
	staged_at Timestamp,
	reserved_at Timestamp,
	completed_at Timestamp,
	FOREIGN KEY (user_id) REFERENCES users(id)
);

-- Add up migration script here
CREATE TABLE reservations (
	id BIGSERIAL PRIMARY KEY,
	user_id BIGINT NOT NULL,
	library_id VARCHAR(255) NOT NULL,
	book_id VARCHAR(255) NOT NULL,
	status VARCHAR(255) NOT NULL,
	staging_at Timestamp NOT NULL,
	staged_at Timestamp,
	reserved_at Timestamp,
	completed_at Timestamp,
	FOREIGN KEY (user_id) REFERENCES users(id)
);

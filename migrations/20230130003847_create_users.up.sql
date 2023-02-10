-- Add up migration script here
CREATE TABLE users (
	id BIGSERIAL PRIMARY KEY NOT NULL,
	email VARCHAR(255) UNIQUE NOT NULL,
	password VARCHAR(255) NOT NULL,
	fullname VARCHAR(255) NOT NULL,
	address VARCHAR(255) NOT NULL
);

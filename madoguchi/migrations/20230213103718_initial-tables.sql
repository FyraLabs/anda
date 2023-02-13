CREATE TABLE repos (
	name	VARCHAR(255) PRIMARY KEY,
	link	VARCHAR(255) NOT NULL,
	gh		VARCHAR(255) NOT NULL
);

CREATE TABLE pkgs (
	name	VARCHAR(255) NOT NULL,
	repo	VARCHAR(255) REFERENCES repos(name),
	verl	VARCHAR(255) NOT NULL,
	arch	VARCHAR(225) NOT NULL,
	dirs	VARCHAR(255) NOT NULL,
	PRIMARY KEY (name, repo, verl, arch)
--	build	INT UNIQUE REFERENCES builds(id)
);

CREATE TABLE builds (
	id		SERIAL PRIMARY KEY,
	epoch	TIMESTAMP NOT NULL,
	pname	VARCHAR(255) NOT NULL,
	pverl	VARCHAR(255) NOT NULL,
	parch	VARCHAR(255) NOT NULL,
	repo	VARCHAR(255) REFERENCES repos(name),
	link	VARCHAR(255) NOT NULL,
	CONSTRAINT fk_pkg FOREIGN KEY (pname, repo, pverl, parch) REFERENCES pkgs (name, repo, verl, arch)

);

ALTER TABLE pkgs ADD build INT UNIQUE REFERENCES builds(id);

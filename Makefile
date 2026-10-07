SHELL := /bin/bash

.PHONY: dev build test smoke db-setup clean

dev:
	npm run dev

build:
	npm run build

test:
	npm test

smoke:
	npm run smoke

db-setup:
	npm run db

clean:
	rm -rf node_modules apps/*/node_modules packages/*/node_modules apps/*/dist

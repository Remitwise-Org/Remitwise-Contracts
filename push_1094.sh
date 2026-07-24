#!/bin/bash
git config user.name "Immaculate0606"
git config user.email "Immaculate0606@users.noreply.github.com"

git checkout -b docs/events-versioning-adr || git checkout docs/events-versioning-adr

git add docs/events-versioning.md
git add README.md
git add docs/EVENTS.md

git commit -m "docs: Add ADR for why events are versioned via a _v2 suffix

Closes #1094"

git push origin HEAD

#!/bin/bash
for t in "$@"; do echo "##### $t"; diff target/conformance/$t.expected target/conformance/$t.actual | head -${N:-24}; done

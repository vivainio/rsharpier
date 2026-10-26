#!/bin/bash
# pushes with the vivainio account without changing the active gh account
T=$(gh auth token --user vivainio) && git -c credential.helper= -c "http.extraheader=Authorization: Basic $(printf 'x-access-token:%s' "$T" | base64 -w0)" push origin main 2>&1 | tail -1

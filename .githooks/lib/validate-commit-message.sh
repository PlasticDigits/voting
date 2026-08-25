#!/usr/bin/env bash
# Validate commit message body: no email addresses, no "author" keyword.
# Policy (body only — subject line is not checked):
#   - Reject lines containing an email address
#   - Reject lines containing the word "author" (matches Co-authored-by, Author:, etc.)

COMMIT_MSG_EMAIL_RE='[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}'
COMMIT_MSG_AUTHOR_RE='(^|[^[:alnum:]_-])author([^[:alnum:]_-]|$)'

_commit_msg_line_is_violation() {
  local line=$1
  local reason_var=$2
  if echo "$line" | grep -qE "$COMMIT_MSG_EMAIL_RE"; then
    printf -v "$reason_var" '%s' "email address"
    return 0
  fi
  if echo "$line" | grep -qiE "$COMMIT_MSG_AUTHOR_RE"; then
    printf -v "$reason_var" '%s' "keyword \"author\""
    return 0
  fi
  return 1
}

_commit_msg_split_subject_body() {
  local msgfile=$1
  local subject_var=$2
  local body_var=$3
  local parsed_subject parsed_body
  parsed_subject=$(head -n 1 "$msgfile" || true)
  parsed_body=$(awk 'NR==1 { next } !started && /^$/ { next } { started=1; print }' "$msgfile")
  printf -v "$subject_var" '%s' "$parsed_subject"
  printf -v "$body_var" '%s' "$parsed_body"
}

_commit_msg_collect_body_violations() {
  local body=$1
  local -n out_violations=$2

  out_violations=()
  if [[ -z "${body//[$'\t ']/}" ]]; then
    return 0
  fi

  local line n=0 reason
  while IFS= read -r line || [[ -n "$line" ]]; do
    n=$((n + 1))
    if _commit_msg_line_is_violation "$line" reason; then
      out_violations+=("body line $n ($reason): $line")
    fi
  done <<<"$body"
}

strip_commit_message_file() {
  local msgfile=$1
  local subject body
  _commit_msg_split_subject_body "$msgfile" subject body

  if [[ -z "${body//[$'\t ']/}" ]]; then
    return 0
  fi

  local -a clean_lines=()
  local line reason stripped=0
  while IFS= read -r line || [[ -n "$line" ]]; do
    if _commit_msg_line_is_violation "$line" reason; then
      stripped=1
    else
      clean_lines+=("$line")
    fi
  done <<<"$body"

  if [[ "$stripped" -ne 1 ]]; then
    return 0
  fi

  {
    printf '%s\n' "$subject"
    if [[ ${#clean_lines[@]} -gt 0 ]]; then
      printf '\n'
      printf '%s\n' "${clean_lines[@]}"
    fi
  } >"$msgfile"
}

validate_commit_message() {
  local msgfile=$1

  if [[ ! -f "$msgfile" ]]; then
    echo "validate_commit_message: missing message file: $msgfile" >&2
    return 1
  fi

  local subject body
  _commit_msg_split_subject_body "$msgfile" subject body

  local -a violations=()
  _commit_msg_collect_body_violations "$body" violations

  if [[ ${#violations[@]} -eq 0 ]]; then
    return 0
  fi

  echo "commit-msg: REJECTED — commit message body must not contain email addresses or the word \"author\"." >&2
  echo "  (Co-authored-by / Author trailers are not allowed.)" >&2
  printf '  - %s\n' "${violations[@]}" >&2
  echo "  Remove the offending lines from the message body. Do not use git commit --no-verify." >&2
  return 1
}

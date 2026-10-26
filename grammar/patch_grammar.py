#!/usr/bin/env python3
"""Applies our fixes to the upstream tree-sitter-c-sharp 0.23.5 grammar.js, then regenerates.

usage: patch_grammar.py <path to pristine grammar.js> <tree-sitter binary>
"""
import re, subprocess, sys

src, ts = sys.argv[1], sys.argv[2]
s = open(src).read()

def rep(old, new, count=1):
    global s
    if old not in s:
        raise SystemExit("patch target not found:\n" + old)
    s = s.replace(old, new, count)

# `x is null || y` must be `(x is null) || y`: only operators binding tighter than the
# relational operators belong to a constant pattern
rep("constant_pattern: $ => choice(\n      $.binary_expression,",
    "constant_pattern: $ => choice(\n      alias($._pattern_binary_expression, $.binary_expression),")
rep("    // Invocation with name - creates conflict", """    _pattern_binary_expression: $ => choice(...[
      ['>>', PREC.SHIFT],
      ['>>>', PREC.SHIFT],
      ['<<', PREC.SHIFT],
      ['+', PREC.ADD],
      ['-', PREC.ADD],
      ['*', PREC.MULT],
      ['/', PREC.MULT],
      ['%', PREC.MULT],
    ].map(([operator, precedence]) =>
      prec.left(precedence, seq(
        field('left', $.expression),
        // @ts-ignore
        field('operator', operator),
        field('right', $.expression),
      )),
    )),

    // Invocation with name - creates conflict""")
rep("    [$.constant_pattern, $.non_lvalue_expression],",
    "    [$.constant_pattern, $.non_lvalue_expression],\n    [$._pattern_binary_expression, $.binary_expression],")

# newer syntax
rep("      'unsafe',\n      'virtual',", "      'unsafe',\n      'virtual',\n      'closed',")
rep("      seq('class', optional('?')),\n      'struct',", "      seq('class', optional('?')),\n      'struct',\n      seq('allows', 'ref', 'struct'),")
rep("      repeat($._attribute_list),\n      optional('extern'),\n      '~',", "      repeat($._attribute_list),\n      repeat($.modifier),\n      '~',")
rep("      $.object_creation_expression,\n      $.parenthesized_expression,\n    ),",
    "      $.object_creation_expression,\n      $.parenthesized_expression,\n      $.checked_expression,\n    ),")
rep("    [$.constant_pattern, $.non_lvalue_expression],",
    "    [$.constant_pattern, $.non_lvalue_expression],\n    [$.non_lvalue_expression, $._expression_statement_expression],")

# `a ?? b?.c` is `a ?? (b?.c)`: the target of ?. can not be a binary expression
rep("""    conditional_access_expression: $ => prec.right(PREC.CONDITIONAL, seq(
      field('condition', $.expression),""", """    _access_target: $ => choice(
      'base',
      $.interpolated_string_expression,
      $.conditional_access_expression,
      $.literal,
      $.invocation_expression,
      $.postfix_unary_expression,
      $.parenthesized_expression,
      $.object_creation_expression,
      $.checked_expression,
      $.default_expression,
      $.sizeof_expression,
      $.typeof_expression,
      $.array_creation_expression,
      $.implicit_object_creation_expression,
      $.implicit_array_creation_expression,
      $.anonymous_object_creation_expression,
      $.lvalue_expression,
    ),

    conditional_access_expression: $ => prec.left(PREC.POSTFIX, seq(
      field('condition', $._access_target),""")

# `new List<(int a, int b)> { }` is a generic name, not `new List < (...) > { }`
rep("generic_name: $ => seq($.identifier, $.type_argument_list),",
    "generic_name: $ => prec.dynamic(20, seq($.identifier, $.type_argument_list)),")

# list patterns: `[.., x]` and `[..]` are slice patterns
rep("commaSep1(choice($.pattern, '..')),", "commaSep1(choice($.pattern, $.slice_pattern)),")
rep("    list_pattern: $ => prec.right(seq(", "    slice_pattern: $ => prec.right(seq('..', optional($.pattern))),\n\n    list_pattern: $ => prec.right(seq(")

rep("      $.and_pattern,\n      $.list_pattern,", "      $.and_pattern,\n      prec.dynamic(5, $.list_pattern),")

# `params` parameters are parameters
rep("sep(choice($.parameter, $._parameter_array), ','),", "sep(choice($.parameter, alias($._parameter_array, $.parameter)), ','),", 2)

# null-conditional assignment: a?.b = c
rep("      'this',\n      $.member_access_expression,\n      $.tuple_expression,", "      'this',\n      $.member_access_expression,\n      $.conditional_access_expression,\n      $.tuple_expression,")

# `x is A.B or C.D` must not parse as `(x is A.B or C).D`: only primary expressions can be accessed
rep("field('expression', choice($.expression, $.predefined_type, $._name)),", "field('expression', choice($._access_target, $.predefined_type, $._name)),")
rep("""    element_access_expression: $ => prec(PREC.POSTFIX, seq(
      field('expression', $.expression),""", """    element_access_expression: $ => prec(PREC.POSTFIX, seq(
      field('expression', $._access_target),""")

# and, or and not are only keywords inside of patterns, they can still be names in most places
rep("    identifier: $ => choice(", """    _name_ext: $ => choice($.identifier, alias(choice('and', 'or', 'not'), $.identifier)),

    identifier: $ => choice(""")
rep("      choice(field('name', $.identifier), $.tuple_pattern),", "      choice(field('name', $._name_ext), $.tuple_pattern),")
rep("      field('left', choice($.identifier, $.tuple_pattern)),", "      field('left', choice($._name_ext, $.tuple_pattern)),")
rep("      optional(field('type', $.type)),\n      field('name', $.identifier),\n      'in',", "      optional(field('type', $.type)),\n      field('name', $._name_ext),\n      'in',")
rep("      'let',\n      $.identifier,", "      'let',\n      $._name_ext,")
rep("seq(optional(field('type', $.type)), $.identifier, 'in', $.expression)", "seq(optional(field('type', $.type)), $._name_ext, 'in', $.expression)")
rep("      alias($.identifier, $.implicit_parameter),", "      alias($._name_ext, $.implicit_parameter),")
rep("      'this',\n      $.member_access_expression,\n      $.conditional_access_expression,\n      $.tuple_expression,\n      $._simple_name,", "      'this',\n      $.member_access_expression,\n      $.conditional_access_expression,\n      $.tuple_expression,\n      $._simple_name,\n      alias(choice('and', 'or', 'not'), $.identifier),")
rep("      optional($._parameter_type_with_modifiers),\n      field('name', $.identifier),", "      optional($._parameter_type_with_modifiers),\n      field('name', $._name_ext),")
rep("      field('name', $.identifier),\n      optional(seq('=', $.expression)),\n    ),\n\n    bracketed_argument_list", "      field('name', $._name_ext),\n      optional(seq('=', $.expression)),\n    ),\n\n    bracketed_argument_list")

open("tree-sitter-c-sharp/grammar.js", "w").write(s)

for i in range(30):
    r = subprocess.run([ts, "generate"], capture_output=True, text=True, cwd="tree-sitter-c-sharp")
    out = r.stdout + r.stderr
    if "Unresolved conflict" not in out:
        print(out[-800:])
        print("generated after", i, "conflict fixes")
        break
    m = re.findall(r"Add a conflict for these rules: (.*)", out)
    rules = re.findall(r"`([^`]+)`", m[0])
    entry = "    [" + ", ".join("$." + x for x in rules) + "],"
    s = open("tree-sitter-c-sharp/grammar.js").read()
    s = s.replace("  conflicts: $ => [\n", "  conflicts: $ => [\n" + entry + "\n", 1)
    open("tree-sitter-c-sharp/grammar.js", "w").write(s)
    print("added conflict", entry)
else:
    sys.exit(out[:2000])

# edge-cases/mdx/multiline-comment.mdx

## Option 1

`````json
{"printWidth":80}
`````

### Diff

`````diff
===================================================================
--- prettier
+++ oxfmt
@@ -1,14 +1,14 @@
 # Multi-line comments
 
-{/_
-multi line
-comment
-_/}
+{/*
+  multi line
+  comment
+*/}
 
 - item
 
-  {/_
-  nested
-  _/}
+  {/*
+    nested
+  */}
 
 {/* single line */}

`````

### Actual (oxfmt)

`````mdx
# Multi-line comments

{/*
  multi line
  comment
*/}

- item

  {/*
    nested
  */}

{/* single line */}

`````

### Expected (prettier)

`````mdx
# Multi-line comments

{/_
multi line
comment
_/}

- item

  {/_
  nested
  _/}

{/* single line */}

`````

## Option 2

`````json
{"printWidth":100,"proseWrap":"always"}
`````

### Diff

`````diff
===================================================================
--- prettier
+++ oxfmt
@@ -1,9 +1,14 @@
 # Multi-line comments
 
-{/_ multi line comment _/}
+{/*
+  multi line
+  comment
+*/}
 
 - item
 
-  {/_ nested _/}
+  {/*
+    nested
+  */}
 
 {/* single line */}

`````

### Actual (oxfmt)

`````mdx
# Multi-line comments

{/*
  multi line
  comment
*/}

- item

  {/*
    nested
  */}

{/* single line */}

`````

### Expected (prettier)

`````mdx
# Multi-line comments

{/_ multi line comment _/}

- item

  {/_ nested _/}

{/* single line */}

`````

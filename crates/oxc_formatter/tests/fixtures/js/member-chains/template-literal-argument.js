const a = env.DB.prepare(/* sql */ `
  SELECT 1
`).bind(1);
const b = db.prepare(`
  SELECT 1
`).bind(1);
const c = prepare(`
  SELECT 1
`).bind(1);
const d = env.DB.prepare(`
  SELECT 1
`);
const e = a.b(`
  x
`)(y).c();
const f = a.b(`
  x
`).c.d(`
  y
`).e();
foo.bar(`
  x
`).baz;
const g = a.b?.(`
  x
`).c();

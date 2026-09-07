local Exchange = dofile("lightroom/Lighttable.lrplugin/Exchange.lua")
local id = "12345678-1234-1234-1234-123456789ABC"
local function document(row)
    return "LIGHTTABLE\t1\tabcdef\nuuid\tfile_uuid\tfilename_hex\tvirtual_copy\tbase_rating\trating\tbase_pick\tpick\n" .. row .. "\n"
end
local row = id .. "\t" .. id .. "\t612e646e67\t0\t3\t0\t1\t0"
local entry = Exchange.parse(document(row))[1]
assert(entry.filename == "a.dng" and entry.rating == 0 and entry.pick == 0)
local current = {uuid=id, filename="a.dng", virtual_copy=false, rating=3, pick=1}
local plan = assert(Exchange.plan(entry, current))
assert(plan.rating == 0 and plan.pick == 0)
current.rating = 0; current.pick = 0
assert(next(assert(Exchange.plan(entry, current))) == nil) -- idempotent repeat
current.rating = 4
assert(Exchange.plan(entry, current) == nil) -- conflict
current.rating = 3; current.virtual_copy = true
assert(Exchange.plan(entry, current) == nil)
assert(Exchange.plan(entry, nil) == nil)
assert(not pcall(Exchange.parse, document(row .. "\n" .. row)))
assert(not pcall(Exchange.parse, document(row:gsub("\t3\t0", "\t3\t9"))))
assert(not pcall(Exchange.parse, "return os.execute('bad')"))
assert(not pcall(Exchange.parse, document(row:gsub("612e646e67", "2e2e2f612e646e67"))))
if arg[1] then
    local f=assert(io.open(arg[1],"rb"));local entries=Exchange.parse(f:read("*a"));f:close()
    assert(#entries>0);print("Rust export parsed by Lua: " .. #entries .. " changes")
end
print("Exchange tests passed: clearing, conflicts, repeated imports, identity, malformed input")

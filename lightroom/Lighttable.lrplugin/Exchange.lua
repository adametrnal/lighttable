-- Pure data parsing and conflict logic; no Lua code is evaluated from the exchange file.
local M = {}
local columns = "uuid\tfile_uuid\tfilename_hex\tvirtual_copy\tbase_rating\trating\tbase_pick\tpick"
local function split(line)
    local values = {}
    for value in (line .. "\t"):gmatch("(.-)\t") do values[#values + 1] = value end
    return values
end
local function uuid(s)
    return #s == 36 and s:match("^%x%x%x%x%x%x%x%x%-%x%x%x%x%-%x%x%x%x%-%x%x%x%x%-%x%x%x%x%x%x%x%x%x%x%x%x$")
end
local function number(s, low, high, optional)
    if optional and s == "-" then return nil end
    assert(s:match("^%-?%d+$"), "Invalid numeric field")
    local value = tonumber(s)
    assert(value >= low and value <= high, "Rating or flag outside permitted range")
    return value
end
function M.parse(text)
    assert(#text <= 32 * 1024 * 1024, "Changes file exceeds 32 MiB")
    local lines = {}
    for rawLine in (text .. "\n"):gmatch("([^\n]*)\n") do
        local line = rawLine:gsub("\r$", "")
        if line ~= "" then lines[#lines + 1] = line end
    end
    assert(lines[1] and lines[1]:match("^LIGHTTABLE\t1\t%x+$"), "Not a Lighttable version 1 changes file")
    assert(lines[2] == columns, "Unexpected changes-file columns")
    local result, seen = {}, {}
    for i = 3, #lines do
        local f = split(lines[i])
        assert(#f == 8 and uuid(f[1]) and uuid(f[2]), "Invalid photo identity at line " .. i)
        assert(not seen[f[1]], "Duplicate photo UUID")
        seen[f[1]] = true
        assert(#f[3] > 0 and #f[3] <= 4096 and #f[3] % 2 == 0 and f[3]:match("^%x+$"), "Invalid encoded filename")
        local filename = f[3]:gsub("%x%x", function(pair) return string.char(tonumber(pair, 16)) end)
        assert(not filename:find("%z") and not filename:find("[/\\]"), "Filename must be a basename")
        assert(f[4] == "0" or f[4] == "1", "Invalid virtual-copy value")
        local entry = {
            uuid = f[1], file_uuid = f[2], filename = filename, virtual_copy = f[4] == "1",
            base_rating = number(f[5], 0, 5), rating = number(f[6], 0, 5, true),
            base_pick = number(f[7], -1, 1), pick = number(f[8], -1, 1, true),
        }
        assert(entry.rating ~= nil or entry.pick ~= nil, "Empty change entry")
        result[#result + 1] = entry
    end
    return result
end
function M.plan(entry, current)
    if not current then return nil, "Photo UUID not found" end
    if current.uuid ~= entry.uuid or current.filename ~= entry.filename or current.virtual_copy ~= entry.virtual_copy then
        return nil, "Photo identity, filename, or virtual-copy mismatch"
    end
    local result = {}
    for _, spec in ipairs({ { "rating", "base_rating" }, { "pick", "base_pick" } }) do
        local field, baseline = spec[1], spec[2]
        if entry[field] ~= nil then
            local value = current[field] or 0
            if value ~= entry[field] then
                if value ~= entry[baseline] then return nil, "Conflict in " .. field end
                result[field] = entry[field]
            end
        end
    end
    return result
end
return M

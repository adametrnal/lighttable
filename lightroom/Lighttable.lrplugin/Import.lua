local LrApplication = import "LrApplication"
local LrDialogs = import "LrDialogs"
local LrTasks = import "LrTasks"
local LrPathUtils = import "LrPathUtils"
local LrProgressScope = import "LrProgressScope"
local Exchange = dofile(LrPathUtils.child(_PLUGIN.path, "Exchange.lua"))

local function current(photo)
    if not photo then return nil end
    return {
        uuid = photo:getRawMetadata("uuid"),
        filename = LrPathUtils.leaf(photo:getRawMetadata("path")),
        virtual_copy = photo:getRawMetadata("isVirtualCopy") == true,
        rating = photo:getRawMetadata("rating") or 0,
        pick = photo:getRawMetadata("pickStatus") or 0,
    }
end

LrTasks.startAsyncTask(function()
    local applied, skipped, already = 0, 0, 0
    local progress
    local ok, failure = LrTasks.pcall(function()
        local paths = LrDialogs.runOpenPanel {
            title = "Import Lighttable selections", canChooseFiles = true, canChooseDirectories = false,
            allowsMultipleSelection = false, fileTypes = { "tsv" },
        }
        if not paths or not paths[1] then return end
        local file, err = io.open(paths[1], "rb")
        assert(file, err)
        local text = file:read(32 * 1024 * 1024 + 1)
        file:close()
        local entries = Exchange.parse(text or "")
        local catalog = LrApplication.activeCatalog()
        local candidates, notes = {}, {}
        progress = LrProgressScope { title = "Reviewing Lighttable selections" }
        for i, entry in ipairs(entries) do
            if progress:isCanceled() then return end
            local photo = catalog:findPhotoByUuid(entry.uuid)
            local plan, reason = Exchange.plan(entry, current(photo))
            if not plan then
                skipped = skipped + 1
                notes[#notes + 1] = entry.filename .. ": " .. reason
            elseif next(plan) == nil then already = already + 1
            else candidates[#candidates + 1] = { photo = photo, entry = entry } end
            progress:setPortionComplete(i, math.max(#entries, 1))
            if i % 200 == 0 then LrTasks.yield() end
        end
        progress:done()
        local summary = string.format("Catalog: %s\n\n%d photos ready to update\n%d already match\n%d conflicts or unresolved photos will be skipped\n\nOnly ratings and pick/reject flags are changed.\nThis experimental importer should be tested on a disposable catalog first.", catalog:getPath(), #candidates, already, skipped)
        if LrDialogs.confirm("Import Lighttable selections?", summary, "Apply selections", "Cancel") ~= "ok" then return end
        progress = LrProgressScope { title = "Applying Lighttable selections" }
        for first = 1, #candidates, 200 do
            if progress:isCanceled() then break end
            local chunkApplied, chunkSkipped, chunkNotes = 0, 0, {}
            -- Re-check inside the write gate, after the user has reviewed the import.
            catalog:withWriteAccessDo("Import Lighttable selections", function()
                for j = first, math.min(first + 199, #candidates) do
                    local candidate = candidates[j]
                    local plan, reason = Exchange.plan(candidate.entry, current(candidate.photo))
                    if plan then
                        if plan.rating ~= nil then candidate.photo:setRawMetadata("rating", plan.rating) end
                        if plan.pick ~= nil then candidate.photo:setRawMetadata("pickStatus", plan.pick) end
                        chunkApplied = chunkApplied + 1
                    else
                        chunkSkipped = chunkSkipped + 1
                        chunkNotes[#chunkNotes + 1] = candidate.entry.filename .. ": " .. reason
                    end
                end
            end)
            applied = applied + chunkApplied
            skipped = skipped + chunkSkipped
            for _, note in ipairs(chunkNotes) do notes[#notes + 1] = note end
            progress:setPortionComplete(math.min(first + 199, #candidates), math.max(#candidates, 1))
            LrTasks.yield()
        end
        progress:done()
        local report = string.format("Applied: %d\nAlready matched: %d\nSkipped: %d\n\n", applied, already, skipped) .. table.concat(notes, "\n")
        local reportPath = paths[1] .. ".report-" .. os.date("%Y%m%d-%H%M%S") .. ".txt"
        local output = io.open(reportPath, "w")
        if output then output:write(report); output:close() end
        LrDialogs.message("Lighttable import finished", string.format("%d applied, %d already matched, %d skipped.\n\n%s\n\nClose Lightroom, copy the updated catalog back to Linux, and refresh Lighttable to reconcile imported selections.", applied, already, skipped, output and ("Report: " .. reportPath) or report:sub(1, 4000)), "info")
    end)
    if progress then progress:done() end
    if not ok then LrDialogs.message("Lighttable import stopped", string.format("%d photos were committed before the error. Re-importing the same file is safe for values that already match.\n\n%s", applied, tostring(failure)), "critical") end
end)

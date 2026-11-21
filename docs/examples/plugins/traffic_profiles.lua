-- plugins/traffic_profiles.lua: optional AI traffic density by time of day.
-- Put this file in the game's plugins folder, then adjust it in Plugins > Settings.
-- Never changes a multiplayer session or the proportion of timetable vehicles.

local settings = omsi.plugin.settings({
  { key = "enabled", type = "bool", label = "Use traffic time profiles", default = true },
  { key = "morning_cars", type = "number", label = "Morning peak (06-09)", default = 140, min = 0, max = 500, step = 10 },
  { key = "day_cars", type = "number", label = "Daytime (09-17)", default = 80, min = 0, max = 500, step = 10 },
  { key = "evening_cars", type = "number", label = "Evening peak (17-20)", default = 150, min = 0, max = 500, step = 10 },
  { key = "night_cars", type = "number", label = "Night (20-06)", default = 30, min = 0, max = 500, step = 10 },
  { key = "weekend_enabled", type = "bool", label = "Weekend traffic preset", default = false },
  { key = "weekend_cars", type = "number", label = "Weekend traffic", default = 70, min = 0, max = 500, step = 10 },
  { key = "holiday_today", type = "bool", label = "Treat this date as a public holiday", default = false },
  { key = "holiday_cars", type = "number", label = "Holiday traffic", default = 50, min = 0, max = 500, step = 10 },
}, "Traffic by time")

local last_map, last_period, last_count

local function profile(seconds, date)
  -- The API gives the weekday, but not the map's Holidays.txt or active
  -- Chrono holiday rules. Holiday override is therefore explicitly manual.
  if settings.holiday_today then return "holiday", settings.holiday_cars end
  if settings.weekend_enabled and type(date) == "table" and
     type(date.weekday) == "number" and date.weekday >= 6 then
    return "weekend", settings.weekend_cars
  end
  local hour = math.floor(seconds / 3600) % 24
  if hour >= 6 and hour < 9 then return "morning", settings.morning_cars end
  if hour >= 9 and hour < 17 then return "day", settings.day_cars end
  if hour >= 17 and hour < 20 then return "evening", settings.evening_cars end
  return "night", settings.night_cars
end

local function apply()
  if not settings.enabled then
    last_period, last_count = nil, nil
    return
  end
  local info = omsi.info()
  -- A client or host should not replace the shared session's traffic settings.
  if not info or info.multiplayer then return end
  local clock = omsi.world.time()
  if type(clock) ~= "number" then return end
  local date = omsi.world.date()
  local period, requested = profile(clock, date)
  if type(requested) ~= "number" or requested ~= requested then return end
  local count = math.max(0, math.min(500, math.floor(requested + 0.5)))
  local map = info.map_path or info.map
  local day = date and string.format("%04d-%02d-%02d", date.year or 0, date.month or 0, date.day or 0) or "unknown"
  local key = period .. ":" .. day
  if map == last_map and key == last_period and count == last_count then return end
  local current, share = omsi.traffic.density()
  if type(current) ~= "number" then return end
  if current == count or omsi.traffic.set_density(count, share) then
    last_map, last_period, last_count = map, key, count
  end
end

omsi.on("setting", function(key, value)
  settings[key] = value
  last_period = nil
  apply()
end)

-- The game clock can be changed by the player; the schedule follows it on the next tick.
omsi.every(10, apply)
apply()

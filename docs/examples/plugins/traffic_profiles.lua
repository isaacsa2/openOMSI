-- plugins/traffic_profiles.lua: optional AI traffic density by time of day.
-- Put this file in the game's plugins folder, then adjust it in Plugins > Settings.
-- Never changes a multiplayer session or the proportion of timetable vehicles.

local settings = omsi.plugin.settings({
  { key = "enabled", type = "bool", label = "Use traffic time profiles", default = true },
  { key = "morning_cars", type = "number", label = "Morning peak (06-09)", default = 140, min = 0, max = 500, step = 10 },
  { key = "day_cars", type = "number", label = "Daytime (09-17)", default = 80, min = 0, max = 500, step = 10 },
  { key = "evening_cars", type = "number", label = "Evening peak (17-20)", default = 150, min = 0, max = 500, step = 10 },
  { key = "night_cars", type = "number", label = "Night (20-06)", default = 30, min = 0, max = 500, step = 10 },
}, "Traffic by time")

local last_map, last_period, last_count

local function profile(seconds)
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
  local period, requested = profile(clock)
  local count = math.max(0, math.min(500, math.floor(requested + 0.5)))
  local map = info.map_path or info.map
  if map == last_map and period == last_period and count == last_count then return end
  local current, share = omsi.traffic.density()
  if type(current) ~= "number" then return end
  if current == count or omsi.traffic.set_density(count, share) then
    last_map, last_period, last_count = map, period, count
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

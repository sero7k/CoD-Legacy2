#include <std_include.hpp>
#include "loader/component_loader.hpp"

#include "game/game.hpp"

#include "filesystem.hpp"
#include "console.hpp"

#include <algorithm>
#include <set>
#include <utils/hook.hpp>
#include <utils/io.hpp>
#include <utils/memory.hpp>

#define MAX_ARENAS 64

namespace arena
{
	namespace
	{
		std::recursive_mutex arena_mutex;

		bool parse_arena(const std::string& path)
		{
			std::lock_guard<std::recursive_mutex> _0(arena_mutex);

			std::string buffer{};
			if (utils::io::read_file(path, &buffer) && !buffer.empty())
			{
				*game::ui_num_arenas += game::GameInfo_ParseArenas(buffer.data(), MAX_ARENAS - *game::ui_num_arenas,
					&game::ui_arena_infos[*game::ui_num_arenas]);
				return true;
			}

			if (!game::DB_XAssetExists(game::ASSET_TYPE_RAWFILE, path.data()) ||
				game::DB_IsXAssetDefault(game::ASSET_TYPE_RAWFILE, path.data()))
			{
				return false;
			}

			const auto* rawfile = game::DB_FindXAssetHeader(game::ASSET_TYPE_RAWFILE, path.data(), 0).rawfile;
			const auto len = game::DB_GetRawFileLen(rawfile);

			const auto rawfile_buffer = utils::memory::get_allocator()->allocate_array<char>(len);
			const auto _1 = gsl::finally([&]
			{
				utils::memory::get_allocator()->free(rawfile_buffer);
			});

			game::DB_GetRawBuffer(rawfile, rawfile_buffer, len);
			*game::ui_num_arenas += game::GameInfo_ParseArenas(rawfile_buffer, MAX_ARENAS - *game::ui_num_arenas,
				&game::ui_arena_infos[*game::ui_num_arenas]);
			
			return true;
		}

		std::string lower(std::string s)
		{
			std::ranges::transform(s, s.begin(), [](const unsigned char c) { return static_cast<char>(std::tolower(c)); });
			return s;
		}

		// Whether an arena for this map is already in the list (info strings
		// are "\key\value" pairs).
		bool arena_loaded(const std::string& map)
		{
			const auto needle = "\\map\\" + lower(map);
			for (auto i = 0; i < *game::ui_num_arenas; i++)
			{
				const char* info = game::ui_arena_infos[i];
				if (!info)
				{
					continue;
				}

				const auto text = lower(info);
				const auto at = text.find(needle);
				if (at == std::string::npos)
				{
					continue;
				}

				const auto end = at + needle.size();
				if (end == text.size() || text[end] == '\\')
				{
					return true;
				}
			}

			return false;
		}

		bool is_map_name(const std::string& name)
		{
			return !name.empty() && std::ranges::all_of(name, [](const unsigned char c)
			{
				return std::isalnum(c) || c == '_';
			});
		}

		// The folder names under hmw-usermaps in every search path, sorted.
		std::vector<std::string> list_usermaps()
		{
			std::set<std::string> names;
			for (const auto& search_path : filesystem::get_search_paths())
			{
				std::error_code ec{};
				for (const auto& entry : std::filesystem::directory_iterator(
					     std::filesystem::path(search_path) / "hmw-usermaps", ec))
				{
					std::error_code entry_ec{};
					if (!entry.is_directory(entry_ec))
					{
						continue;
					}

					const auto name = entry.path().filename().string();
					if (is_map_name(name))
					{
						names.insert(name);
					}
				}
			}

			return {names.begin(), names.end()};
		}

		void load_usermap_arena(const std::string& map, const bool report = true)
		{
			if (arena_loaded(map))
			{
				return;
			}

			if (*game::ui_num_arenas >= MAX_ARENAS)
			{
				console::warn("[arena] map selection is full (%d): %s not listed\n", MAX_ARENAS, map.data());
				return;
			}

			if (!parse_arena("hmw-usermaps/" + map + "/" + map + ".arena") && report)
			{
				console::warn("[arena] no arena file for usermap %s\n", map.data());
			}
		}

		void load_arenas_stub()
		{
			*game::ui_num_arenas = 0;
			*game::ui_arena_buf_pos = 0;

			parse_arena("mp/basemaps.arena");

			// every folder in hmw-usermaps is a map in the map selection
			for (const auto& name : list_usermaps())
			{
				load_usermap_arena(name);
			}

			// read all usermap arenas for map list
			if (std::filesystem::exists("usermaps"))
			{
				for (const auto& entry : std::filesystem::directory_iterator("usermaps"))
				{
					if (!entry.is_directory())
					{
						continue;
					}

					auto mapname = entry.path().filename().string();
					auto arena_path = entry.path().string() + "/" + mapname + ".arena";

					parse_arena(arena_path);
				}
			}
		}
	}

	class component final : public component_interface
	{
	public:
		void post_unpack() override
		{
			if (!game::environment::is_mp())
			{
				return;
			}

			// load custom arenas
			utils::hook::jump(0x4DE030_b, load_arenas_stub);
		}
	};
}

REGISTER_COMPONENT(arena::component)

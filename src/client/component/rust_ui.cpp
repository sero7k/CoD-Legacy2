#include <std_include.hpp>
#include "loader/component_loader.hpp"

#include "scheduler.hpp"
#include "console.hpp"
#include "command.hpp"

#include "game/game.hpp"
#include "game/dvars.hpp"

#include "rust/codlegacy.h"

// The in-match HUD is drawn by Rust (rust/codlegacy/src/ui.rs) through the
// engine's own 2D commands, once a frame in the renderer's pass. Lua feeds it
// (game:rustset) and hides H1's HUD widgets.
namespace rust_ui
{
	namespace
	{
		// Fonts are registered at a few sizes only; text of another size is
		// scaled from the nearest.
		constexpr int font_sizes[] = {16, 20, 24, 28, 34, 40, 48};

		struct picked_font
		{
			game::Font_s* font;
			float scale;
		};

		picked_font pick_font(const int size)
		{
			auto best = font_sizes[0];
			for (const auto candidate : font_sizes)
			{
				if (std::abs(candidate - size) < std::abs(best - size))
				{
					best = candidate;
				}
			}

			return {game::R_RegisterFont("fonts/fira_mono_regular.ttf", best),
				static_cast<float>(size) / static_cast<float>(best)};
		}

		void copy_color(float (&out)[4], const float* rgba)
		{
			for (auto i = 0; i < 4; i++)
			{
				out[i] = rgba ? rgba[i] : 1.0f;
			}
		}

		void fill_rect(const float x, const float y, const float w, const float h, const float* rgba)
		{
			static const auto white = game::Material_RegisterHandle("white");
			float color[4];
			copy_color(color, rgba);
			game::R_AddCmdDrawStretchPic(x, y, w, h, 0.0f, 0.0f, 1.0f, 1.0f, color, white);
		}

		void draw_text(const char* text, const float x, const float y, const int size, const float* rgba)
		{
			const auto [font, scale] = pick_font(size);
			if (!font || !text)
			{
				return;
			}

			float color[4];
			copy_color(color, rgba);
			game::R_AddCmdDrawText(text, 0x7FFFFFFF, font, x, y, scale, scale, 0.0f, color, 6);
		}

		float text_width(const char* text, const int size)
		{
			const auto [font, scale] = pick_font(size);
			if (!font || !text)
			{
				return 0.0f;
			}

			return static_cast<float>(game::R_TextWidth(text, 0x7FFFFFFF, font)) * scale;
		}

		void draw_image(const char* material, const float x, const float y, const float w, const float h,
		                const float* rgba)
		{
			if (!material)
			{
				return;
			}

			const auto mat = game::Material_RegisterHandle(material);
			if (!mat)
			{
				return;
			}

			float color[4];
			copy_color(color, rgba);
			game::R_AddCmdDrawStretchPic(x, y, w, h, 0.0f, 0.0f, 1.0f, 1.0f, color, mat);
		}

		void host_log(const char* line)
		{
			console::info("%s", line ? line : "");
		}

		void host_exec(const char* command)
		{
			if (command && *command)
			{
				command::execute(command);
			}
		}

		bool host_dvar_string(const char* name, char* out, const size_t cap)
		{
			if (!name || !out || cap == 0)
			{
				return false;
			}

			out[0] = '\0';
			const auto* dvar = game::Dvar_FindVar(name);
			if (!dvar)
			{
				return false;
			}

			std::string text;
			switch (dvar->type)
			{
			case game::dvar_type::boolean:
			case game::dvar_type::boolean_hashed:
				text = dvar->current.enabled ? "1" : "0";
				break;
			case game::dvar_type::integer:
			case game::dvar_type::integer_hashed:
			case game::dvar_type::enumeration:
				text = std::to_string(dvar->current.integer);
				break;
			case game::dvar_type::value:
			case game::dvar_type::value_hashed:
				text = std::to_string(dvar->current.value);
				break;
			case game::dvar_type::string:
				text = dvar->current.string ? dvar->current.string : "";
				break;
			default:
				return false;
			}

			strncpy_s(out, cap, text.c_str(), _TRUNCATE);
			return true;
		}

		game::dvar_t* music_volume_dvar;

		// 0 silent (loading), 1 menu, 3 match; sent to Rust when it changes.
		int music_context()
		{
			if (*game::mp::client_state != nullptr && game::CL_IsCgameInitialized() && !game::VirtualLobby_Loaded())
			{
				return 3;
			}

			return game::VirtualLobby_Loaded() ? 1 : 0;
		}

		void update_music()
		{
			static int last_context = -1;
			static float last_volume = -1.0f;

			const auto context = music_context();
			if (context != last_context)
			{
				last_context = context;
				codlegacy_music_context(context);
			}

			const auto volume = music_volume_dvar ? music_volume_dvar->current.value : 0.0f;
			if (volume != last_volume)
			{
				last_volume = volume;
				codlegacy_music_volume(volume);
			}
		}

		void draw_menu()
		{
			if (!codlegacy_menu_is_open())
			{
				return;
			}

			static const codlegacy_ui_api api{fill_rect, draw_text, text_width, draw_image};
			const auto* place = game::ScrPlace_GetViewPlacement();
			codlegacy_menu_frame(&api, place->realViewportSize[0], place->realViewportSize[1]);
		}

		void draw_hud()
		{
			if (!game::CL_IsCgameInitialized() || game::VirtualLobby_Loaded() || *game::mp::client_state == nullptr)
			{
				return;
			}

			static const codlegacy_ui_api api{fill_rect, draw_text, text_width, draw_image};
			const auto* place = game::ScrPlace_GetViewPlacement();
			codlegacy_ui_frame(&api, place->realViewportSize[0], place->realViewportSize[1]);
		}
	}

	class component final : public component_interface
	{
	public:
		void pre_destroy() override
		{
			codlegacy_music_stop();
		}

		void post_unpack() override
		{
			if (game::environment::is_dedi() || !game::environment::is_mp())
			{
				return;
			}

			static const codlegacy_host_api host{host_log, host_exec, host_dvar_string};
			codlegacy_init(&host);

			music_volume_dvar = dvars::register_float("legacy_music_volume", 0.6f, 0.0f, 1.0f, game::DVAR_FLAG_SAVED,
				"CoD Legacy's music volume (music/menu, music/lobby, music/match beside the exe)");
			scheduler::loop(update_music, scheduler::pipeline::async, 250ms);

			command::add("legacy_menu", []()
			{
				codlegacy_menu_toggle();
			});

			scheduler::loop(draw_hud, scheduler::pipeline::renderer);
			scheduler::loop(draw_menu, scheduler::pipeline::renderer);
		}
	};
}

REGISTER_COMPONENT(rust_ui::component)

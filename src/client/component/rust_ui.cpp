#include <std_include.hpp>
#include "loader/component_loader.hpp"

#include "scheduler.hpp"

#include "game/game.hpp"

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

		void draw_hud()
		{
			if (!game::CL_IsCgameInitialized() || game::VirtualLobby_Loaded() || *game::client_state == nullptr)
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
		void post_unpack() override
		{
			if (game::environment::is_dedi() || !game::environment::is_mp())
			{
				return;
			}

			scheduler::loop(draw_hud, scheduler::pipeline::renderer);
		}
	};
}

REGISTER_COMPONENT(rust_ui::component)

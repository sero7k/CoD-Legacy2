#pragma once

#include <cstddef>

// CoD Legacy's Rust core (rust/codlegacy): a static library linked into the
// client. See rust/codlegacy/src/lib.rs and ui.rs.

struct codlegacy_ui_api
{
	void (*fill_rect)(float x, float y, float w, float h, const float* rgba);
	// y is the text's baseline, size its height in pixels.
	void (*draw_text)(const char* text, float x, float y, int size, const float* rgba);
	float (*text_width)(const char* text, int size);
	void (*draw_image)(const char* material, float x, float y, float w, float h, const float* rgba);
};

// The engine as Rust sees it (rust/codlegacy/src/host.rs): handed over once
// by codlegacy_init.
struct codlegacy_host_api
{
	void (*log)(const char* line);
	void (*exec)(const char* command);
	// Copies a dvar's value as text into out (cap bytes, always NUL-terminated).
	bool (*dvar_string)(const char* name, char* out, size_t cap);
};

extern "C"
{
	bool codlegacy_init(const codlegacy_host_api* api);

	// The music (rust/codlegacy/src/music): what the game is doing (0 silent,
	// 1 menu, 2 lobby, 3 match; the first call starts the music thread) and
	// how loud (0..1).
	void codlegacy_music_context(int context);
	void codlegacy_music_volume(float volume);
	void codlegacy_music_stop();

	// The Legacy menu (rust/codlegacy/src/menu.rs): an overlay over the game.
	void codlegacy_menu_toggle();
	bool codlegacy_menu_is_open();
	// An engine key going down; true when the menu took it.
	bool codlegacy_menu_key(int code);
	void codlegacy_menu_frame(const codlegacy_ui_api* api, float width, float height);

	const char* codlegacy_version();
	float codlegacy_bo2_sprint_speed_scale();
	bool codlegacy_add(int a, int b, int* out);

	// The in-match HUD: values from Lua in, one draw call a frame out.
	void codlegacy_ui_set(const char* key, const char* value);
	bool codlegacy_ui_frame(const codlegacy_ui_api* api, float width, float height);
}

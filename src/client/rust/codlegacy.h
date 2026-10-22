#pragma once

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

extern "C"
{
	const char* codlegacy_version();
	float codlegacy_bo2_sprint_speed_scale();
	bool codlegacy_add(int a, int b, int* out);

	// The in-match HUD: values from Lua in, one draw call a frame out.
	void codlegacy_ui_set(const char* key, const char* value);
	bool codlegacy_ui_frame(const codlegacy_ui_api* api, float width, float height);
}

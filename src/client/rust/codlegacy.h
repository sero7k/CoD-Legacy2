#pragma once

// CoD Legacy's Rust core (rust/codlegacy): a static library linked into the
// client. See rust/codlegacy/src/lib.rs.
extern "C"
{
	const char* codlegacy_version();
	float codlegacy_bo2_sprint_speed_scale();
	bool codlegacy_add(int a, int b, int* out);
}

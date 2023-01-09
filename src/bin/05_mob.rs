/*!
Protohackers Problem 05: Stealing Boguscoin for the Mob

Proxy upstream Budget Chat at `chat.protohackers.com:16963`
to change all boguscoin addresses to Tony's.

Tony's address: 7YWHMfk9JZe0LM0g1ZauHuiSxhI

A valid boguscoin address satsfies all of the following:

  * it starts with a "7"
  * it consists of at least 26, and at most 35, alphanumeric characters
  * it starts at the start of a chat message, or is preceded by a space
  * it ends at the end of a chat message, or is followed by a space
*/

static TONYS_BC_ADDR: &[u8] = b"7YWHMfk9JZe0LM0g1ZauHuiSxhI";

/// Scan a message for a boguscoin address, and return its span if found.
fn find_address(buff: &[u8]) -> Option<(usize, usize)> {
    
}
License-Identifier: HGL
Copyright (C) The Usecode Authors (see AUTHORS)

usecode - Container for agents
==============================

Hi! usecode gives your AI agents a real Linux workspace: machines they can
spin up, tools they can run, and an API they can call.

The easiest way to use it is usecode.dev. It's already set up and kept
running, so your agents can start working today:

  1. Sign in at https://usecode.dev and get an API key.
  2. Hand it to your agent. For Claude Code that's one line:

       claude mcp add usecode -e USECODE_MCP_API_KEY=<your key> -- uc-agent-mcp
Cursor, Codex, Gemini CLI, opencode and friends, see:
https://github.com/evgnomon/usecode/blob/master/docs/coding-agents.md
Cursor, Codex, Gemini CLI, opencode and friends: see docs/coding-agents.md.

Upgrades, backups and uptime are on us, not your weekend.


Running it yourself
-------------------

All the code is here under the HGL General License (COPYING), and it's the
same code usecode.dev runs. On a Debian or Ubuntu box:

  bash <(curl -fsSL https://raw.githubusercontent.com/evgnomon/usecode/refs/heads/master/play.sh)

Then run `uc help`. Self-hosting is on your own time; if you'd like help
setting it up or running it for your team, that's a paid engagement and
we're happy to talk: https://usecode.dev


Contributing
------------

Patches are welcome. See CONTRIBUTING.md (commits need a Signed-off-by line)
and CODE_OF_CONDUCT.md.


No Warranty
-----------

The following disclaimers must keep being prominently displayed in the
documentation and any other materials for the Covered Work or a Derivative
Work:

EXCEPT WHEN OTHERWISE STATED IN WRITING, THE COPYRIGHT HOLDERS AND/OR OTHER
PARTIES PROVIDE THE COVERED WORK "AS IS" WITHOUT IMPLIED WARRANTIES OF FITNESS
FOR A PARTICULAR PURPOSE, NON-INFRINGEMENT, MERCHANTABILITY AND TITLE, AND ANY
OTHER KIND OF EXPRESSED OR IMPLIED WARRANTIES.

IN NO EVENT SHALL ANY COPYRIGHT HOLDER, OR ANY OTHER PARTY WHO MAY MODIFY
AND/OR REDISTRIBUTE THE PROGRAM AS PERMITTED ABOVE BE LIABLE FOR ANY DIRECT,
INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING,
BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE,
LOSS OF GOODWILL, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED
AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT
(INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THE
COVERED WORK OR AS A RESULT OF OUR LICENSE, EVEN IF ADVISED OF THE POSSIBILITY
OF SUCH DAMAGE.

Nevertheless, you retain the option to extend offers of support, warranties,
indemnities, or other liability obligations and/or rights in alignment with
this License. Such offers may be provided in exchange for a fee, at your
discretion. This provision allows you to engage in commercial transactions by
offering additional services or assurances while remaining compliant with the
terms of this License.

# TheBrain Project Notes

## Agent Role

In this project, the assistant acts as a project manager.

When feature development is requested, the assistant must first read the current code and documentation, clarify unclear decisions with the user, and produce a detailed development plan. After the plan is confirmed, the assistant should delegate implementation work to subagents when the available environment supports it.

## README Maintenance

`README.md` is the current source of truth for the project. It should describe the latest project state, current architecture, known requirements, completed goals, and unfinished goals.

`README.md` is not a changelog. Do not record every individual change there unless the change affects the current project state, architecture, requirements, or goals.


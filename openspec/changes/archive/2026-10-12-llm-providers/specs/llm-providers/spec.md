# Spec Delta

## Purpose

Lets users configure, test and choose the large language model providers that power the assistant, including cloud APIs and local models.

## ADDED Requirements

### Requirement: Supported provider kinds
The IDE SHALL support three provider kinds: Anthropic, Google Gemini, and OpenAI-compatible with a configurable base URL, with presets for OpenAI, Ollama, LM Studio and OpenRouter.

#### Scenario: Ollama preset
- **WHEN** the user adds a profile from the Ollama preset
- **THEN** the kind is OpenAI-compatible, the base URL is `http://localhost:11434/v1`, and no API key is required

### Requirement: Profile configuration
Each LLM profile SHALL store a name, kind, base URL, model, temperature, maximum output tokens and request timeout; API keys SHALL be stored in the operating system credential store.

#### Scenario: Key stored securely
- **WHEN** the user saves an Anthropic profile with an API key
- **THEN** the key is retrievable for requests and does not appear in the application database or logs

### Requirement: Model discovery
The profile editor SHALL fetch and list available models from the provider, and SHALL allow typing a model name manually when listing fails or the model is not listed.

#### Scenario: List local models
- **WHEN** the user opens the model picker for a reachable Ollama profile
- **THEN** the locally installed models are listed

#### Scenario: Manual model entry
- **WHEN** model listing fails with an error
- **THEN** the error is shown and the user can still type a model name and save

### Requirement: Test a profile
The profile editor SHALL offer a "Test" action that sends a short prompt, streams the reply into the dialog, and reports time to first token, total time and token usage when provided.

#### Scenario: Successful test
- **WHEN** the user tests a working profile
- **THEN** the reply text streams in and the time to first token and total time are shown

#### Scenario: Invalid key
- **WHEN** the user tests a profile with an invalid API key
- **THEN** an authentication error is shown with the provider's message and no key material

### Requirement: Capability detection
For each profile the IDE SHALL determine whether streaming, tool calling and structured JSON output are supported, using known defaults per provider and model and allowing the user to override them.

#### Scenario: Override tool calling
- **WHEN** the user disables tool calling for a local model profile
- **THEN** the assistant does not send tool definitions to that profile

### Requirement: Default and per-connection profiles
The user SHALL choose a default LLM profile and MAY override it per connection.

#### Scenario: Per-connection override
- **WHEN** the default profile is a cloud model and connection "Prod" overrides it with a local model
- **THEN** assistant requests made while working on "Prod" use the local model

### Requirement: Streaming responses
Responses from all provider kinds SHALL be streamed incrementally to the caller as they are generated.

#### Scenario: Incremental tokens
- **WHEN** a streaming-capable profile generates a reply of several sentences
- **THEN** the caller receives multiple incremental text events before the final event

### Requirement: CLI profile test
The headless CLI SHALL test a saved LLM profile and print the reply and timings.

#### Scenario: CLI LLM test
- **WHEN** the user runs the CLI LLM test command for a working profile
- **THEN** it prints the streamed reply and timings and exits with status 0

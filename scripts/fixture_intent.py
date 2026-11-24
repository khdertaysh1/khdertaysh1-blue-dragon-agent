"""Handle the coding agent's tool-free follow-up phase in action fixtures."""
import json


def reply_to_work_intent(handler, packet):
    if packet.get('tools') or not any(
        message['role'] == 'system' and 'REQUEST_NEW_WORK' in message.get('content', '')
        for message in packet['messages']
    ):
        return False
    message = {'role': 'assistant', 'content': 'REQUEST_NEW_WORK'}
    body = json.dumps({'choices': [{'message': message}]}).encode()
    handler.send_response(200)
    handler.send_header('Content-Type', 'application/json')
    handler.send_header('Content-Length', str(len(body)))
    handler.end_headers()
    handler.wfile.write(body)
    return True

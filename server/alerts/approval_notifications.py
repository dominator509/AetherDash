"""Approval prompt delivery without placing enforcement in the channel."""

from server.alerts.approvals import ApprovalRecord


async def send_approval_prompt(record: ApprovalRecord, reference: str) -> str:
    if record.requires_step_up:
        body = (
            f"AETHER approval {record.id}: open the authenticated AETHER client "
            f"and enter reference {reference} to complete fresh step-up."
        )
    elif record.channel == "sms":
        body = (
            f"AETHER paper approval {record.id}. Reply APPROVE {reference} or "
            f"REJECT {reference}. Expires at {record.expires_at.isoformat()}."
        )
    else:
        body = (
            f"AETHER paper approval {record.id}: open the authenticated AETHER client "
            f"and enter reference {reference}. Expires at {record.expires_at.isoformat()}."
        )
    if record.channel == "sms":
        from connectors.comms.twilio.sender import send_message as send_sms

        return await send_sms(body)
    if record.channel == "email":
        from connectors.comms.email.sender import send_message as send_email

        return await send_email("AETHER approval required", body)
    raise ValueError("unsupported approval notification channel")

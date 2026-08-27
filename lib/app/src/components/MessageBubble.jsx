import { formatMessageText } from '../utils/formatText'

function formatTime(ts) {
  return new Date(ts).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
}

export default function MessageBubble({ message }) {
  const isUser = message.sender === 'user'
  return (
    <div className={`bubble-row${isUser ? ' from-user' : ''}`}>
      <div className={`bubble${isUser ? ' user' : ' agent'}`}>
        {message.type === 'image' ? (
          <>
            <img className="bubble-image" src={message.imageUrl} alt={message.caption || 'image'} loading="lazy" />
            {message.caption && (
              <p
                className="bubble-caption"
                dangerouslySetInnerHTML={{ __html: formatMessageText(message.caption) }}
              />
            )}
          </>
        ) : (
          <p dangerouslySetInnerHTML={{ __html: formatMessageText(message.text) }} />
        )}
        <span className="bubble-time">{formatTime(message.ts)}</span>
      </div>
    </div>
  )
}

import { useHandleSignInCallback } from "@logto/react";

const AuthCallback = () => {
  const { isLoading } = useHandleSignInCallback(() => {
    // Navigate to root path when finished
  });

  // When it's working in progress
  if (isLoading) {
    return <div>Redirecting...</div>;
  }

  return <h1></h1>;
};

export default AuthCallback;
